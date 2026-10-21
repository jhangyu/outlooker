//! RawItem → NormalizedItem (Part 01 §5, Part 04 §4.1, §5.4–§5.8). Pure, panic-safe per item.
pub mod attachments;
pub mod charset;
pub mod participants;
pub mod rtf;
pub mod sanitize;
pub mod text;

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use rayon::prelude::*;

use crate::dto::import::{ImportErrorSeverity, ImportErrorStage};
use crate::importer::dedupe;
use crate::importer::types::{
    BodySource, InlineImageOut, ItemError, NormalizedItem, NormalizedMail, Outcome, RawContent, RawItem, RawMail,
};
use crate::importer::webp::{self, ImageIn, ImageOut, KnownHashes};
use sanitize::{InlineSource, Sanitized};

pub const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
pub const SNIPPET_CHARS: usize = 200;

pub struct NormalizeCtx { pub webp_quality: u8, pub known: Arc<KnownHashes> }

pub fn normalize(raw: RawItem, ctx: &NormalizeCtx) -> NormalizedItem {
    let (seq, pst_item_id, pst_folder_id) = (raw.seq, raw.pst_item_id, raw.pst_folder_id);
    let raw_bytes = raw.approx_bytes();
    let folder_path = raw.folder_path.clone();
    let result = catch_unwind(AssertUnwindSafe(|| normalize_inner(raw, ctx)));
    match result {
        Ok((outcome, errors)) => NormalizedItem { seq, pst_item_id, pst_folder_id, raw_bytes, outcome, errors },
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            let err = ItemError::new(ImportErrorStage::Html, ImportErrorSeverity::ItemSkipped, &folder_path, pst_item_id, &format!("panic: {msg}"));
            NormalizedItem { seq, pst_item_id, pst_folder_id, raw_bytes, outcome: Outcome::Skipped, errors: vec![err] }
        }
    }
}

fn normalize_inner(raw: RawItem, ctx: &NormalizeCtx) -> (Outcome, Vec<ItemError>) {
    match raw.content {
        RawContent::NonMail => (Outcome::NonMail, Vec::new()),
        RawContent::Unreadable { stage, message } => (
            Outcome::Skipped,
            vec![ItemError::new(stage, ImportErrorSeverity::ItemSkipped, &raw.folder_path, raw.pst_item_id, &message)],
        ),
        RawContent::Mail(m) => {
            let mut errors = Vec::new();
            let mail = normalize_mail(&m, ctx, &raw.folder_path, raw.pst_item_id, &mut errors);
            (Outcome::Mail(Box::new(mail)), errors)
        }
    }
}

fn degraded(errors: &mut Vec<ItemError>, stage: ImportErrorStage, folder: &str, item: i64, msg: &str) {
    errors.push(ItemError::new(stage, ImportErrorSeverity::Degraded, folder, item, msg));
}

fn normalize_mail(m: &RawMail, ctx: &NormalizeCtx, folder: &str, item: i64, errors: &mut Vec<ItemError>) -> NormalizedMail {
    // cid map: normalized content id → attachment index (only attachments with bytes).
    let cid_map: HashMap<String, usize> = m
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.data.is_some())
        .filter_map(|(i, a)| a.content_id.as_deref().map(|c| (sanitize::normalize_cid(c), i)))
        .filter(|(c, _)| !c.is_empty())
        .collect();

    // Body selection, Part 04 §5.5.
    let mut sanitized: Option<(BodySource, Sanitized)> = None;
    if let Some(html) = m.html_body.as_deref().filter(|b| !b.is_empty()) {
        let decoded = charset::decode_html(html, m.internet_codepage);
        if decoded.len() > MAX_HTML_BYTES {
            degraded(errors, ImportErrorStage::Html, folder, item, "html body larger than 16 MB, not sanitized");
        } else {
            sanitized = Some((BodySource::Html, sanitize::sanitize(&decoded, &cid_map)));
        }
    }
    if sanitized.is_none()
        && let Some(rtf) = m.rtf_body.as_deref().filter(|r| rtf::is_html_encapsulated(r))
    {
        let html = rtf::deencapsulate_html(rtf);
        sanitized = Some((BodySource::RtfHtml, sanitize::sanitize(&html, &cid_map)));
    }
    let plain = m.plain_body.as_deref().filter(|p| !p.trim().is_empty());

    let (body_source, mut body_html, refs) = match sanitized {
        Some((src, s)) => (src, Some(s.html), s.refs),
        None if plain.is_some() => (BodySource::Text, None, Vec::new()),
        None if m.rtf_body.is_some() => (BodySource::RtfText, None, Vec::new()),
        None => (BodySource::None, None, Vec::new()),
    };

    // Inline images (Part 04 §5.7): convert every referenced image in parallel.
    let mut referenced: HashSet<usize> = HashSet::new();
    let inputs: Vec<(i32, Option<String>, ImageIn)> = refs
        .into_iter()
        .filter_map(|r| match r.source {
            InlineSource::Cid { cid, attachment } => {
                referenced.insert(attachment);
                let bytes = m.attachments[attachment].data.clone()?;
                Some((r.ordinal, Some(cid), ImageIn { bytes }))
            }
            InlineSource::Data(bytes) => Some((r.ordinal, None, ImageIn { bytes })),
        })
        .collect();
    let converted: Vec<(i32, Option<String>, ImageOut)> = inputs
        .into_par_iter()
        .map(|(ordinal, cid, input)| (ordinal, cid, webp::convert(&input, ctx.webp_quality, &ctx.known)))
        .collect();
    let mut inline_images = Vec::new();
    for (ordinal, content_id, out) in converted {
        match out {
            ImageOut::Encoded { source_hash, image } => {
                inline_images.push(InlineImageOut { ordinal, content_id, source_hash, blob: Some(image) })
            }
            ImageOut::Known { source_hash } => inline_images.push(InlineImageOut { ordinal, content_id, source_hash, blob: None }),
            ImageOut::Tiny => body_html = body_html.map(|h| sanitize::drop_src(&h, ordinal)),
            ImageOut::Rejected { message } => {
                degraded(errors, ImportErrorStage::Image, folder, item, &message);
                body_html = body_html.map(|h| sanitize::drop_src(&h, ordinal));
            }
        }
    }
    inline_images.sort_by_key(|i| i.ordinal);

    // body_text (search text).
    let raw_text = if let Some(p) = plain {
        p.to_string()
    } else if let Some(h) = body_html.as_deref() {
        match text::html_to_text(h) {
            Ok(t) => t,
            Err(e) => {
                degraded(errors, ImportErrorStage::Html, folder, item, &format!("html to text failed: {e}"));
                String::new()
            }
        }
    } else if let Some(r) = m.rtf_body.as_deref() {
        rtf::to_text(r)
    } else {
        String::new()
    };
    let body_text = text::clean_text(&raw_text);
    let snippet = text::snippet(&body_text);

    let p = participants::participants(m);
    let attachments = m
        .attachments
        .iter()
        .enumerate()
        .filter(|(i, _)| !referenced.contains(i))
        .map(|(_, a)| attachments::attachment_out(a))
        .collect();
    let subject = m.subject.clone().unwrap_or_default();
    let dedupe_sender = p
        .sender
        .as_ref()
        .map(|a| a.email.clone())
        .or_else(|| p.sender_name.as_ref().map(|n| n.trim().to_lowercase()))
        .unwrap_or_default();
    let key = dedupe::key(
        m.internet_message_id.as_deref(),
        m.transport_headers.as_deref(),
        &dedupe_sender,
        m.client_submit_time,
        &subject,
        &body_text,
    );
    NormalizedMail {
        dedupe_key: key.key,
        dedupe_kind: key.kind,
        message_id: key.message_id,
        subject,
        sent_at: m.client_submit_time,
        received_at: m.delivery_time,
        sender: p.sender,
        sender_name: p.sender_name,
        sender_search: p.sender_search,
        recipients: p.recipients,
        recipients_search: p.recipients_search,
        to_preview: p.to_preview,
        body_source,
        body_html,
        body_text,
        snippet,
        attachments,
        inline_images,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::types::{DedupeKind, RawAttachment};
    use crate::importer::webp::tests::png;

    fn golden(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    fn ctx() -> NormalizeCtx { NormalizeCtx { webp_quality: 80, known: Arc::new(KnownHashes::new()) } }

    fn item(m: RawMail) -> RawItem {
        RawItem { seq: 1, pst_item_id: 100, pst_folder_id: 1, folder_path: "Inbox".into(), content: RawContent::Mail(Box::new(m)) }
    }

    fn mail_of(item: NormalizedItem) -> NormalizedMail {
        match item.outcome { Outcome::Mail(m) => *m, other => panic!("expected mail, got {other:?}") }
    }

    fn inline_mail() -> RawMail {
        RawMail {
            subject: Some("Inline".into()),
            html_body: Some(golden("inline.html").into_bytes()),
            attachments: vec![
                RawAttachment { index: 0, content_id: Some("<logo@x>".into()), long_filename: Some("logo.png".into()), data: Some(png(40, 20)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 1, content_id: Some("pixel@x".into()), long_filename: Some("pixel.png".into()), data: Some(png(1, 1)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 2, content_id: Some("unused@x".into()), long_filename: Some("unused.png".into()), data: Some(png(8, 8)), attach_method: Some(1), ..Default::default() },
                RawAttachment { index: 3, long_filename: Some("report.pdf".into()), data_size: Some(1234), attach_method: Some(1), ..Default::default() },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn golden_inline() {
        let m = mail_of(normalize(item(inline_mail()), &ctx()));
        assert_eq!(m.body_html.as_deref(), Some(golden("inline.expected.html").trim_end_matches('\n')));
        assert_eq!(m.inline_images.iter().map(|i| i.ordinal).collect::<Vec<_>>(), vec![0]);
        assert_eq!(m.inline_images[0].content_id.as_deref(), Some("logo@x"));
        // referenced attachments (logo, pixel) are inline; unused cid + pdf stay attachments
        assert_eq!(m.attachments.iter().map(|a| a.file_name.as_str()).collect::<Vec<_>>(), vec!["unused.png", "report.pdf"]);
    }

    #[test]
    fn one_pixel_image_loses_src() {
        let m = mail_of(normalize(item(inline_mail()), &ctx()));
        let html = m.body_html.unwrap();
        assert!(html.contains(r#"<img alt="pixel">"#), "{html}");
        assert!(!html.contains("outlooker-inline:1"));
    }

    #[test]
    fn plain_body_wins_for_text_and_html_kept() {
        let m = RawMail {
            html_body: Some(golden("plain.html").into_bytes()),
            plain_body: Some("Plain   body\r\n\r\n\r\ntext".into()),
            ..Default::default()
        };
        let n = mail_of(normalize(item(m), &ctx()));
        assert_eq!(n.body_source, BodySource::Html);
        assert_eq!(n.body_text, "Plain body\n\ntext");
    }

    #[test]
    fn rtf_paths_and_none() {
        let fromhtml = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/fromhtml.rtf")).unwrap();
        let n = mail_of(normalize(item(RawMail { rtf_body: Some(fromhtml), ..Default::default() }), &ctx()));
        assert_eq!(n.body_source, BodySource::RtfHtml);
        assert!(n.body_text.contains("Price: 100€ café"), "{}", n.body_text);
        let plain_rtf = br"{\rtf1\ansi Hello\par world}".to_vec();
        let n = mail_of(normalize(item(RawMail { rtf_body: Some(plain_rtf), ..Default::default() }), &ctx()));
        assert_eq!((n.body_source, n.body_html.is_none(), n.body_text.as_str()), (BodySource::RtfText, true, "Hello\nworld"));
        let n = mail_of(normalize(item(RawMail::default()), &ctx()));
        assert_eq!((n.body_source, n.body_text.as_str()), (BodySource::None, ""));
    }

    #[test]
    fn non_mail_and_unreadable() {
        let mut raw = item(RawMail::default());
        raw.content = RawContent::NonMail;
        assert!(matches!(normalize(raw, &ctx()).outcome, Outcome::NonMail));
        let mut raw = item(RawMail::default());
        raw.content = RawContent::Unreadable { stage: ImportErrorStage::Parse, message: "bad block".into() };
        let n = normalize(raw, &ctx());
        assert!(matches!(n.outcome, Outcome::Skipped));
        assert_eq!(n.errors[0].severity, ImportErrorSeverity::ItemSkipped);
        assert_eq!(n.errors[0].pst_item_id, Some(100));
    }

    #[test]
    fn invariants_over_goldens() {
        let samples = vec![
            RawMail { html_body: Some(golden("plain.html").into_bytes()), sender_smtp_address: Some("A@B.C".into()), ..Default::default() },
            inline_mail(),
            RawMail { html_body: Some(golden("hostile.html").into_bytes()), internet_message_id: Some("<x@y>".into()), ..Default::default() },
            RawMail { rtf_body: Some(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/fromhtml.rtf")).unwrap()), ..Default::default() },
            RawMail { plain_body: Some("p".repeat(500)), ..Default::default() },
            RawMail { rtf_body: Some(br"{\rtf1 x}".to_vec()), ..Default::default() },
            RawMail::default(),
        ];
        for s in samples {
            let m = mail_of(normalize(item(s), &ctx()));
            assert!(m.snippet.chars().count() <= 200);
            for e in m.sender.iter().map(|a| &a.email).chain(m.recipients.iter().filter_map(|r| r.address.as_ref()).map(|a| &a.email)) {
                assert_eq!(e, &e.to_lowercase());
            }
            assert_eq!(m.body_html.is_none(), matches!(m.body_source, BodySource::Text | BodySource::RtfText | BodySource::None));
            assert_eq!(m.dedupe_key.len(), 32);
            assert_eq!(m.message_id.is_some(), m.dedupe_kind == DedupeKind::MessageId);
        }
    }
}
