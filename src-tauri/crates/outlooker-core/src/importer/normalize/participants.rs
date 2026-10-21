//! Part 04 §5.4 sender, recipients and search strings.
use crate::dto::search::RecipientKind;
use crate::importer::types::{AddressIn, AddressType, RawMail, RecipientOut};

pub struct Participants {
    pub sender: Option<AddressIn>,
    pub sender_name: Option<String>,
    pub sender_search: String,
    pub recipients: Vec<RecipientOut>,
    pub recipients_search: String,
    pub to_preview: String,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// trim + lowercase; empty or longer than 2,048 chars → None.
fn address(raw: &str, address_type: AddressType) -> Option<AddressIn> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() || email.chars().count() > 2048 {
        return None;
    }
    Some(AddressIn { email, address_type })
}

fn type_of(t: Option<&str>) -> AddressType {
    match t {
        Some(t) if t.trim().eq_ignore_ascii_case("smtp") => AddressType::Smtp,
        Some(t) if t.trim().eq_ignore_ascii_case("ex") => AddressType::Ex,
        _ => AddressType::Other,
    }
}

/// "Name <addr>" | "Name" | "addr" | "".
pub fn search_text(name: Option<&str>, addr: Option<&str>) -> String {
    match (name, addr) {
        (Some(n), Some(a)) => format!("{n} <{a}>"),
        (Some(n), None) => n.to_string(),
        (None, Some(a)) => a.to_string(),
        (None, None) => String::new(),
    }
}

pub fn participants(m: &RawMail) -> Participants {
    let sender = non_empty(&m.sender_smtp_address)
        .map(|a| (a, AddressType::Smtp))
        .or_else(|| non_empty(&m.sent_representing_smtp_address).map(|a| (a, AddressType::Smtp)))
        .or_else(|| non_empty(&m.sender_email_address).map(|a| (a, type_of(m.sender_address_type.as_deref()))))
        .and_then(|(a, t)| address(a, t));
    let sender_name = non_empty(&m.sender_name).or_else(|| non_empty(&m.sent_representing_name)).map(str::to_string);
    let sender_search = search_text(sender_name.as_deref(), sender.as_ref().map(|a| a.email.as_str()));

    let mut recipients = Vec::new();
    for (i, r) in m.recipients.iter().enumerate() {
        let addr = non_empty(&r.smtp_address)
            .map(|a| (a, AddressType::Smtp))
            .or_else(|| non_empty(&r.email_address).map(|a| (a, type_of(r.address_type.as_deref()))))
            .and_then(|(a, t)| address(a, t));
        let display_name = non_empty(&r.display_name).map(str::to_string);
        if addr.is_none() && display_name.is_none() {
            continue; // DB CHECK: address or name required
        }
        let kind = match r.recipient_type {
            Some(2) => RecipientKind::Cc,
            Some(3) => RecipientKind::Bcc,
            _ => RecipientKind::To,
        };
        let search_text = search_text(display_name.as_deref(), addr.as_ref().map(|a| a.email.as_str()));
        recipients.push(RecipientOut { ordinal: i as i32, kind, address: addr, display_name, search_text });
    }
    let recipients_search = recipients.iter().map(|r| r.search_text.as_str()).collect::<Vec<_>>().join("\n");
    let to_preview = recipients
        .iter()
        .filter(|r| r.kind == RecipientKind::To)
        .take(3)
        .map(|r| r.display_name.clone().or_else(|| r.address.as_ref().map(|a| a.email.clone())).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ");
    Participants { sender, sender_name, sender_search, recipients, recipients_search, to_preview }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::types::RawRecipient;

    #[test]
    fn sender_fallbacks_and_recipients() {
        let m = RawMail {
            sender_email_address: Some(" /O=ORG/CN=Alice ".into()),
            sender_address_type: Some("EX".into()),
            sent_representing_name: Some("Alice Chen".into()),
            recipients: vec![
                RawRecipient { smtp_address: Some("Bob@X.COM".into()), display_name: Some("Bob".into()), recipient_type: Some(1), ..Default::default() },
                RawRecipient { ..Default::default() },
                RawRecipient { email_address: Some("carol@x.com".into()), address_type: Some("SMTP".into()), recipient_type: Some(2), ..Default::default() },
                RawRecipient { display_name: Some("Dave".into()), recipient_type: Some(9), ..Default::default() },
            ],
            ..Default::default()
        };
        let p = participants(&m);
        assert_eq!(p.sender, Some(AddressIn { email: "/o=org/cn=alice".into(), address_type: AddressType::Ex }));
        assert_eq!(p.sender_search, "Alice Chen </o=org/cn=alice>");
        assert_eq!(p.recipients.iter().map(|r| r.ordinal).collect::<Vec<_>>(), vec![0, 2, 3]);
        assert_eq!(p.recipients[1].address.as_ref().unwrap().address_type, AddressType::Smtp);
        assert_eq!(p.recipients[2].kind, RecipientKind::To);
        assert_eq!(p.recipients_search, "Bob <bob@x.com>\ncarol@x.com\nDave");
        assert_eq!(p.to_preview, "Bob, Dave");
    }
}
