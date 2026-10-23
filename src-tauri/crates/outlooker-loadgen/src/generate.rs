//! Synthetic `NormalizedItem`s (spec Part 04 §12.3). Item `i` of source `s` is a pure function of
//! (seed, s, i), so a duplicate is produced by regenerating an earlier slot.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use outlooker_core::dto::search::RecipientKind;
use outlooker_core::importer::dedupe;
use outlooker_core::importer::types::{
    AddressIn, AddressType, AttachmentOut, BodySource, DedupeKind, InlineImageOut, NormalizedItem, NormalizedMail, Outcome,
    RecipientOut,
};
use outlooker_core::importer::webp::KnownHashes;

use crate::corpus::Corpus;
use crate::images::ImagePool;
use crate::rng::Rng;

/// 2026-01-01T00:00:00Z: generated dates fall in the 10 years before it.
pub const DATE_END: u64 = 1_767_225_600;
pub const DATE_SPAN: u64 = 10 * 365 * 86_400;
pub const SENDERS: u64 = 20_000;
pub const RECIPIENT_POOL: u64 = 50_000;
pub const NO_MESSAGE_ID_RATE: f64 = 0.10;
pub const E2E_SUBJECT: &str = "E2E 報價單 inline";

#[derive(Debug, Clone, PartialEq)]
pub struct GenConfig {
    pub messages: u64,
    pub sources: u32,
    pub seed: u64,
    pub msgid_dup_rate: f64,
    pub hash_dup_rate: f64,
    pub image_rate: f64,
    pub image_pool: u32,
    pub logo_pool: u32,
    pub max_image_side: u32,
    pub e2e_message: bool,
}

impl GenConfig {
    pub fn full() -> GenConfig {
        GenConfig { messages: 1_000_000, sources: 10, seed: 42, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.30, image_pool: 50_000, logo_pool: 200, max_image_side: 1600, e2e_message: false }
    }
    pub fn smoke(messages: u64) -> GenConfig {
        GenConfig { messages, sources: 10, seed: 7, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.30, image_pool: 200, logo_pool: 20, max_image_side: 400, e2e_message: false }
    }
    pub fn seed(messages: u64) -> GenConfig {
        GenConfig { messages, sources: 2, seed: 11, msgid_dup_rate: 0.03, hash_dup_rate: 0.005, image_rate: 0.10, image_pool: 100, logo_pool: 10, max_image_side: 400, e2e_message: true }
    }
    /// Items in source `s` (the last source takes the remainder).
    pub fn per_source(&self, s: u32) -> u64 {
        let base = self.messages / self.sources as u64;
        if s + 1 == self.sources { self.messages - base * (self.sources as u64 - 1) } else { base }
    }
}

/// What slot (s, i) holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Fresh,
    /// Same Message-ID (and content) as fresh slot (s, i).
    MessageIdDup { s: u32, i: u64 },
    /// Same sender, time, subject and body as fresh slot (s, i) without Message-ID; body line endings CRLF.
    HashDup { s: u32, i: u64 },
}

pub struct Generator {
    pub cfg: GenConfig,
    pub corpus: Corpus,
    pub images: ImagePool,
}

fn slot_rng(seed: u64, s: u32, i: u64, salt: u64) -> Rng {
    Rng::new(seed.wrapping_mul(1_000_003) ^ ((s as u64) << 40) ^ i ^ salt.wrapping_mul(0x9E37_79B9))
}

impl Generator {
    pub fn new(cfg: GenConfig) -> Generator {
        let images = ImagePool::new(cfg.seed, cfg.logo_pool, cfg.max_image_side);
        Generator { cfg, corpus: Corpus::new(), images }
    }

    fn has_message_id(&self, s: u32, i: u64) -> bool {
        !slot_rng(self.cfg.seed, s, i, 1).chance(NO_MESSAGE_ID_RATE)
    }

    fn is_e2e(&self, s: u32, i: u64) -> bool {
        self.cfg.e2e_message && s == 0 && i == 0
    }

    pub fn slot(&self, s: u32, i: u64) -> Slot {
        if s == 0 || self.is_e2e(s, i) {
            return Slot::Fresh;
        }
        let mut r = slot_rng(self.cfg.seed, s, i, 2);
        let u = r.unit();
        let want_msgid = if u < self.cfg.msgid_dup_rate {
            true
        } else if u < self.cfg.msgid_dup_rate + self.cfg.hash_dup_rate {
            false
        } else {
            return Slot::Fresh;
        };
        // up to 16 candidates in earlier sources; a candidate must be a fresh slot of the wanted key kind
        for _ in 0..16 {
            let cs = r.below(s as u64) as u32;
            let ci = r.below(self.cfg.per_source(cs));
            if self.slot(cs, ci) == Slot::Fresh && !self.is_e2e(cs, ci) && self.has_message_id(cs, ci) == want_msgid {
                return if want_msgid { Slot::MessageIdDup { s: cs, i: ci } } else { Slot::HashDup { s: cs, i: ci } };
            }
        }
        Slot::Fresh
    }

    /// Item `i` (0-based) of source `s`: seq = pst_item_id = i + 1, folder = year of the sent time.
    pub fn item(&self, s: u32, i: u64, known: &KnownHashes) -> NormalizedItem {
        let mail = match self.slot(s, i) {
            Slot::Fresh if self.is_e2e(s, i) => self.e2e_mail(known),
            Slot::Fresh => self.fresh(s, i, known),
            Slot::MessageIdDup { s: bs, i: bi } => self.fresh(bs, bi, known),
            Slot::HashDup { s: bs, i: bi } => {
                let mut m = self.fresh(bs, bi, known);
                m.body_text = m.body_text.replace('\n', "\r\n");
                m
            }
        };
        let year = mail.sent_at.map_or(2025, year_of);
        NormalizedItem { seq: i as i64 + 1, pst_item_id: i as i64 + 1, pst_folder_id: year, raw_bytes: 0, outcome: Outcome::Mail(Box::new(mail)), errors: Vec::new() }
    }

    fn fresh(&self, s: u32, i: u64, known: &KnownHashes) -> NormalizedMail {
        let mut r = slot_rng(self.cfg.seed, s, i, 3);
        let lang = Corpus::lang(&mut r);
        let subject = self.corpus.subject(&mut r, lang);
        // log-normal body: median 2 KB, p99 60 KB (sigma = ln(30) / 2.326), capped at 64 KB
        let len = (2048.0 * (1.462 * r.normal()).exp()).clamp(64.0, 65_536.0) as usize;
        let body_text = self.corpus.text(&mut r, lang, len);
        let sent = UNIX_EPOCH + Duration::from_secs(DATE_END - 1 - r.below(DATE_SPAN));
        let k = r.below(SENDERS);
        let sender_email = format!("sender{k}@loadgen.example");
        let sender_name = format!("Sender {k}");
        let n_rcpt = if r.chance(0.01) { 300 } else { r.range(1, 10) };
        let mut recipients = Vec::with_capacity(n_rcpt as usize);
        for ordinal in 0..n_rcpt as i32 {
            let u = r.below(RECIPIENT_POOL);
            let kind = if ordinal == 0 { RecipientKind::To } else { [RecipientKind::To, RecipientKind::To, RecipientKind::Cc, RecipientKind::Bcc][r.below(4) as usize] };
            let email = format!("user{u}@corp{}.example", u % 50);
            let name = format!("User {u}");
            recipients.push(RecipientOut {
                ordinal,
                kind,
                search_text: format!("{name} <{email}>"),
                address: Some(AddressIn { email, address_type: AddressType::Smtp }),
                display_name: Some(name),
            });
        }
        let attachments = if r.chance(0.25) {
            (0..r.range(1, 3) as i32)
                .map(|a| AttachmentOut { attachment_index: a, file_name: format!("document-{a}.pdf"), size_bytes: r.range(1_000, 5_000_000) as i64, mime_type: "application/pdf".into(), attach_method: 1 })
                .collect()
        } else {
            Vec::new()
        };
        let mut inline_images = Vec::new();
        if r.chance(self.cfg.image_rate) {
            for ordinal in 0..r.range(1, 3) as i32 {
                let index = if r.chance(0.2) { r.below(self.cfg.logo_pool as u64) as u32 } else { self.cfg.logo_pool + r.below(self.cfg.image_pool as u64) as u32 };
                if let Some(img) = self.images.inline(index, ordinal, known) {
                    inline_images.push(img);
                }
            }
        }
        let message_id = self.has_message_id(s, i).then(|| format!("<{s}.{i}.{}@loadgen.example>", self.cfg.seed));
        assemble(subject, body_text, sent, sender_email, sender_name, recipients, attachments, inline_images, message_id.as_deref())
    }

    fn e2e_mail(&self, known: &KnownHashes) -> NormalizedMail {
        let img = self.images.inline(0, 0, known).expect("logo 0 is at least 64 px, so it always encodes");
        let sent = UNIX_EPOCH + Duration::from_secs(DATE_END - 86_400);
        let recipients = vec![RecipientOut {
            ordinal: 0,
            kind: RecipientKind::To,
            address: Some(AddressIn { email: "e2e@loadgen.example".into(), address_type: AddressType::Smtp }),
            display_name: Some("E2E Recipient".into()),
            search_text: "E2E Recipient <e2e@loadgen.example>".into(),
        }];
        assemble(
            E2E_SUBJECT.into(),
            "這是 E2E 測試的報價單，內含一張圖片。\nThis quotation has one inline image.".into(),
            sent,
            "sender0@loadgen.example".into(),
            "Sender 0".into(),
            recipients,
            Vec::new(),
            vec![img],
            Some("<e2e-inline@loadgen.example>"),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    subject: String,
    body_text: String,
    sent: SystemTime,
    sender_email: String,
    sender_name: String,
    recipients: Vec<RecipientOut>,
    attachments: Vec<AttachmentOut>,
    inline_images: Vec<InlineImageOut>,
    message_id: Option<&str>,
) -> NormalizedMail {
    let key = dedupe::key(message_id, None, &sender_email, Some(sent), &subject, &body_text);
    let mut html = String::with_capacity(body_text.len() + 64);
    for p in body_text.split('\n') {
        html.push_str("<p>");
        html.push_str(p);
        html.push_str("</p>");
    }
    for img in &inline_images {
        html.push_str(&format!("<p><img src=\"outlooker-inline:{}\"></p>", img.ordinal));
    }
    let to_preview = recipients
        .iter()
        .filter(|r| r.kind == RecipientKind::To)
        .take(3)
        .filter_map(|r| r.display_name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let recipients_search = recipients.iter().map(|r| r.search_text.as_str()).collect::<Vec<_>>().join("\n");
    let snippet: String = body_text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
    debug_assert!(key.kind == DedupeKind::MessageId || message_id.is_none());
    NormalizedMail {
        dedupe_key: key.key,
        dedupe_kind: key.kind,
        message_id: key.message_id,
        subject,
        sent_at: Some(sent),
        received_at: Some(sent + Duration::from_secs(60)),
        sender_search: format!("{sender_name} <{sender_email}>"),
        sender: Some(AddressIn { email: sender_email, address_type: AddressType::Smtp }),
        sender_name: Some(sender_name),
        recipients,
        recipients_search,
        to_preview,
        body_source: BodySource::Html,
        body_html: Some(html),
        body_text,
        snippet,
        attachments,
        inline_images,
    }
}

/// UTC calendar year of `t` (uses the RFC 3339 formatter: "YYYY-…").
pub fn year_of(t: SystemTime) -> i64 {
    dedupe::format_utc_seconds(t)[..4].parse().expect("four-digit year")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_items_and_duplicate_plan() {
        let g = Generator::new(GenConfig::smoke(10_000));
        let known = KnownHashes::new();
        let a = g.item(3, 17, &known);
        let b = g.item(3, 17, &known);
        let (Outcome::Mail(ma), Outcome::Mail(mb)) = (&a.outcome, &b.outcome) else { panic!() };
        assert_eq!((ma.dedupe_key, &ma.subject, ma.sent_at), (mb.dedupe_key, &mb.subject, mb.sent_at));
        assert!(ma.sender.as_ref().unwrap().email.ends_with("@loadgen.example"));
        let mut dup_msgid = 0;
        let mut dup_hash = 0;
        for s in 0..10 {
            for i in 0..1_000 {
                match g.slot(s, i) {
                    Slot::MessageIdDup { s: bs, i: bi } => {
                        dup_msgid += 1;
                        assert!(bs < s);
                        let (Outcome::Mail(x), Outcome::Mail(y)) = (g.item(s, i, &known).outcome, g.item(bs, bi, &known).outcome) else { panic!() };
                        assert_eq!(x.dedupe_key, y.dedupe_key);
                    }
                    Slot::HashDup { s: bs, i: bi } => {
                        dup_hash += 1;
                        let (Outcome::Mail(x), Outcome::Mail(y)) = (g.item(s, i, &known).outcome, g.item(bs, bi, &known).outcome) else { panic!() };
                        assert_eq!((x.dedupe_key, x.dedupe_kind), (y.dedupe_key, DedupeKind::ContentHash));
                        assert_eq!(x.body_text, y.body_text.replace('\n', "\r\n"), "CRLF copy");
                    }
                    Slot::Fresh => {}
                }
            }
        }
        // 9 of 10 sources can hold duplicates: expected 9,000 × 3% = 270 and 9,000 × 0.5% = 45
        assert!((200..340).contains(&dup_msgid), "{dup_msgid}");
        assert!((20..80).contains(&dup_hash), "{dup_hash}");
    }

    #[test]
    fn e2e_message_in_seed_config() {
        let g = Generator::new(GenConfig::seed(100));
        let known = KnownHashes::new();
        let Outcome::Mail(m) = g.item(0, 0, &known).outcome else { panic!() };
        assert_eq!(m.subject, E2E_SUBJECT);
        assert!(m.body_text.contains("報價單"));
        assert_eq!(m.inline_images.len(), 1);
        assert!(m.inline_images[0].blob.is_some());
        assert!(m.body_html.unwrap().contains("src=\"outlooker-inline:0\""));
        assert_eq!(year_of(UNIX_EPOCH + Duration::from_secs(DATE_END - 1)), 2025);
    }
}
