#![cfg(feature = "test-support")]
//! Search Service tests on seeded rows (spec Part 04 §7, §12.2 rows Search compiler / Search semantics).
use std::time::{Duration, UNIX_EPOCH};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{ConditionGroup, ConditionNode as N, GroupOp, RecipientKind, SearchQuery, SearchRequest};
use outlooker_core::importer::types::{AttachmentOut, BodySource, EncodedImage, InlineImageOut, NormalizedItem, NormalizedMail};
use outlooker_core::importer::webp::source_hash;
use outlooker_core::search::mail::IMAGE_URL_PREFIX;
use outlooker_core::search::{compile, search_page, SearchService};
use outlooker_core::test_support::import_fixtures::{add_recipient, item, mail, seed_mails};
use outlooker_core::test_support::{BoxFuture, TestCluster};

const T0: u64 = 1_600_000_000; // 2020-09-13T12:26:40Z

/// Mail number `n`: unique Message-ID, sent T0 + n hours (or no sent time).
fn m(n: i64, subject: &str, body: &str, sender: &str, sent: Option<u64>) -> NormalizedMail {
    let mut x = mail(subject, body, sender, Some(&format!("<n{n}@seed>")));
    x.sent_at = sent.map(|s| UNIX_EPOCH + Duration::from_secs(s));
    x
}

fn items(mails: Vec<NormalizedMail>) -> Vec<NormalizedItem> {
    mails.into_iter().enumerate().map(|(i, x)| item(i as i64 + 1, i as i64 + 1, x)).collect()
}

fn simple(text: &str) -> SearchRequest {
    SearchRequest { query: SearchQuery::Simple { text: text.into() }, cursor: None, page_size: None }
}

fn dsl(children: Vec<N>, page_size: u32) -> SearchRequest {
    SearchRequest { query: SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children } }, cursor: None, page_size: Some(page_size) }
}

async fn subjects(svc: &SearchService, req: SearchRequest) -> (Vec<String>, bool) {
    let page = svc.search(req).await.unwrap();
    (page.rows.into_iter().map(|r| r.subject).collect(), page.short_query)
}

async fn reference(tc: &TestCluster, subject_like: &str, limit: i64) -> Vec<i64> {
    let a = tc.admin().await;
    a.query("SELECT id FROM messages WHERE subject LIKE $1 ORDER BY sort_at DESC, id DESC LIMIT $2", &[&subject_like, &limit])
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect()
}

/// 100 mails, one hour apart; subject "dense" on all, "sparse" on every 10th (ids 10, 20, …), 5 without sent time.
fn seed_window_set(tc: &TestCluster) -> BoxFuture<'_, ()> {
    Box::pin(async move {
        let mails = (1..=100)
            .map(|i| {
                let subject = if i % 10 == 0 { format!("dense sparse {i}") } else { format!("dense {i}") };
                let sent = if i > 95 { None } else { Some(T0 + i as u64 * 3600) };
                m(i, &subject, "body", "s@x.example", sent)
            })
            .collect();
        seed_mails(&tc.db, "/p/window.pst", items(mails)).await;
    })
}

async fn page_ids(client: &mut tokio_postgres::Client, req: &SearchRequest, w: i64) -> (Vec<i64>, Option<String>, bool) {
    let (page, phase_b) = search_page(client, req, w).await.unwrap();
    (page.rows.iter().map(|r| r.id).collect(), page.next_cursor, phase_b)
}

#[tokio::test]
async fn semantics_short_and_cjk() {
    let tc = TestCluster::start().await;
    seed_mails(
        &tc.db,
        "/p/a.pst",
        items(vec![
            m(1, "a", "請見報價單附件", "s@x.example", Some(T0)),
            m(2, "b", "報價很好", "s@x.example", Some(T0 + 1)),
            m(3, "c", "unrelated", "s@x.example", Some(T0 + 2)),
        ]),
    )
    .await;
    let svc = SearchService::new(tc.db.clone());
    assert_eq!(subjects(&svc, simple("報價")).await, (vec!["b".to_owned(), "a".to_owned()], true));
    assert_eq!(subjects(&svc, simple("報價單")).await, (vec!["a".to_owned()], false));
}

#[tokio::test]
async fn semantics_stem() {
    let tc = TestCluster::start().await;
    seed_mails(
        &tc.db,
        "/p/a.pst",
        items(vec![m(1, "plural", "please pay the invoices", "s@x.example", Some(T0)), m(2, "singular", "one invoice", "s@x.example", Some(T0 + 1))]),
    )
    .await;
    let svc = SearchService::new(tc.db.clone());
    let body = |v: &str| dsl(vec![N::Body { value: v.into() }], 50);
    assert_eq!(subjects(&svc, body("invoice")).await.0, vec!["singular", "plural"]);
    // "invoices" is no substring of "one invoice": only the English stem branch can match it
    assert_eq!(subjects(&svc, body("invoices")).await.0, vec!["singular", "plural"]);
}

#[tokio::test]
async fn semantics_partial_sender_recipient() {
    let tc = TestCluster::start().await;
    let mut a = m(1, "from contoso", "x", "alice.wong@contoso.com", Some(T0));
    a.sender_search = "Alice Wong <alice.wong@contoso.com>".into();
    add_recipient(&mut a, RecipientKind::To, Some("Carol"), "carol@example.org");
    add_recipient(&mut a, RecipientKind::Cc, Some("Bob Lin"), "bob@fabrikam.com");
    let b = m(2, "other", "x", "dave@example.org", Some(T0 + 1));
    seed_mails(&tc.db, "/p/a.pst", items(vec![a, b])).await;
    let svc = SearchService::new(tc.db.clone());
    let hit = vec!["from contoso".to_owned()];
    assert_eq!(subjects(&svc, dsl(vec![N::Sender { value: "Wong".into() }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Sender { value: "contoso".into() }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![] }], 50)).await.0, hit);
    assert_eq!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![RecipientKind::Cc] }], 50)).await.0, hit);
    assert!(subjects(&svc, dsl(vec![N::Recipient { value: "fabrik".into(), kinds: vec![RecipientKind::To] }], 50)).await.0.is_empty());
    assert_eq!(subjects(&svc, simple("bob")).await.0, hit);
}

fn with_attachment(mut x: NormalizedMail) -> NormalizedMail {
    x.attachments.push(AttachmentOut { attachment_index: 0, file_name: "q.pdf".into(), size_bytes: 10, mime_type: "application/pdf".into(), attach_method: 1 });
    x
}

#[tokio::test]
async fn semantics_worked_example() {
    let tc = TestCluster::start().await;
    let in_2023 = 1_688_169_600; // 2023-07-01T00:00:00Z
    let p1 = seed_mails(
        &tc.db,
        "/p/one.pst",
        items(vec![
            with_attachment(m(1, "match sender", "請看報價", "sales@contoso.com", Some(in_2023))),
            m(2, "no attachment", "請看報價", "sales@contoso.com", Some(in_2023)),
            with_attachment(m(3, "wrong year", "請看報價", "sales@contoso.com", Some(1_625_097_600))),
            with_attachment(m(4, "wrong body", "nothing", "sales@contoso.com", Some(in_2023))),
        ]),
    )
    .await;
    let mut by_recipient = with_attachment(m(5, "match recipient", "報價如下", "x@example.org", Some(in_2023 + 60)));
    add_recipient(&mut by_recipient, RecipientKind::To, None, "alice@example.org");
    let p2 = seed_mails(&tc.db, "/p/two.pst", items(vec![by_recipient])).await;
    let p3 = seed_mails(&tc.db, "/p/three.pst", items(vec![with_attachment(m(6, "other source", "請看報價", "sales@contoso.com", Some(in_2023)))])).await;
    assert!(p3 > p2 && p2 > p1);
    let req = dsl(
        vec![
            N::Group { op: GroupOp::Or, children: vec![N::Sender { value: "contoso.com".into() }, N::Recipient { value: "alice".into(), kinds: vec![] }] },
            N::DateRange { from: Some("2022-12-31T16:00:00Z".into()), to: Some("2023-12-31T16:00:00Z".into()) },
            N::Body { value: "報價".into() },
            N::HasAttachment { value: true },
            N::SourcePst { pst_ids: vec![p1, p2] },
        ],
        200,
    );
    let page = SearchService::new(tc.db.clone()).search(req).await.unwrap();
    let got: Vec<&str> = page.rows.iter().map(|r| r.subject.as_str()).collect();
    assert_eq!(got, vec!["match recipient", "match sender"]);
    assert!(page.short_query);
}

#[tokio::test]
async fn literal_percent_underscore_backslash() {
    let tc = TestCluster::start().await;
    let subjects_in = ["100% done", "100 done", "a_b", "axb", r"c\d", "cxd"];
    let mails = subjects_in.iter().enumerate().map(|(i, s)| m(i as i64, s, "x", "s@x.example", Some(T0 + i as u64))).collect();
    seed_mails(&tc.db, "/p/a.pst", items(mails)).await;
    let svc = SearchService::new(tc.db.clone());
    for (needle, only) in [("100%", "100% done"), ("a_b", "a_b"), (r"c\d", r"c\d")] {
        assert_eq!(subjects(&svc, dsl(vec![N::Subject { value: needle.into() }], 50)).await.0, vec![only.to_owned()], "{needle}");
    }
}

#[tokio::test]
async fn count_capped() {
    let tc = TestCluster::start().await;
    // the count reads only `messages`, so seed it directly
    tc.admin()
        .await
        .batch_execute(
            "INSERT INTO messages (dedupe_key, dedupe_kind, body_source, subject, sent_at) \
             SELECT decode(lpad(to_hex(g),64,'0'),'hex'), 'content_hash', 'none', 'capword ' || g, to_timestamp(1600000000+g) \
             FROM generate_series(1,10001) g",
        )
        .await
        .unwrap();
    let svc = SearchService::new(tc.db.clone());
    let q = || SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children: vec![N::Subject { value: "capword".into() }] } };
    let c = svc.count(q()).await.unwrap();
    assert_eq!((c.count, c.capped), (10_000, true));
    tc.admin().await.batch_execute("DELETE FROM messages WHERE id IN (SELECT id FROM messages ORDER BY id LIMIT 2)").await.unwrap();
    let c = svc.count(q()).await.unwrap();
    assert_eq!((c.count, c.capped), (9_999, false));
}

#[tokio::test]
async fn mail_get_unknown_is_not_found() {
    let tc = TestCluster::start().await;
    let err = SearchService::new(tc.db.clone()).mail_get(987_654).await.unwrap_err();
    assert_eq!(err, AppError::NotFound { entity: "mail".into(), id: "987654".into() });
}

fn with_inline_image(mut x: NormalizedMail, bytes: &[u8]) -> NormalizedMail {
    x.body_source = BodySource::Html;
    x.body_html = Some(r#"<p>logo <img src="outlooker-inline:0"><img src="outlooker-inline:1"></p>"#.into());
    x.inline_images.push(InlineImageOut {
        ordinal: 0,
        content_id: Some("logo".into()),
        source_hash: source_hash(bytes),
        blob: Some(EncodedImage { webp: bytes.to_vec(), width: 2, height: 2, source_mime: "image/png".into() }),
    });
    x
}

#[tokio::test]
async fn mail_get_image_urls_platform_form() {
    let tc = TestCluster::start().await;
    let mut x = with_attachment(with_inline_image(m(1, "pic", "logo", "s@x.example", Some(T0)), b"webp-bytes"));
    add_recipient(&mut x, RecipientKind::To, Some("To One"), "to1@x.example");
    add_recipient(&mut x, RecipientKind::Bcc, None, "bcc@x.example");
    add_recipient(&mut x, RecipientKind::Cc, Some("Cc One"), "cc1@x.example");
    seed_mails(&tc.db, "/p/a.pst", items(vec![x])).await;
    let a = tc.admin().await;
    let id: i64 = a.query_one("SELECT id FROM messages", &[]).await.unwrap().get(0);
    let blob: i64 = a.query_one("SELECT id FROM image_blobs", &[]).await.unwrap().get(0);
    let d = SearchService::new(tc.db.clone()).mail_get(id).await.unwrap();
    assert_eq!(d.body_html.unwrap(), format!(r#"<p>logo <img src="{IMAGE_URL_PREFIX}{blob}"><img></p>"#));
    #[cfg(windows)]
    assert!(IMAGE_URL_PREFIX.starts_with("http://mailimg.localhost/"));
    #[cfg(not(windows))]
    assert!(IMAGE_URL_PREFIX.starts_with("mailimg://localhost/"));
    assert_eq!(d.to.len(), 1);
    assert_eq!(d.cc[0].name.as_deref(), Some("Cc One"));
    assert_eq!(d.bcc[0].address.as_deref(), Some("bcc@x.example"));
    assert_eq!(d.sent_at.as_deref(), Some("2020-09-13T12:26:40Z"));
    assert_eq!(d.attachments[0].file_name, "q.pdf");
    assert_eq!((d.occurrences[0].pst_path.as_str(), d.occurrences[0].folder_path.as_str()), ("/p/a.pst", "Inbox"));
}

#[tokio::test]
async fn two_phase_dense_phase_a_only() {
    let tc = TestCluster::from_seed("window", seed_window_set).await;
    let mut client = tc.app().await;
    let (ids, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "dense".into() }], 10), 20).await;
    assert!(!phase_b);
    assert_eq!(ids, reference(&tc, "%dense%", 10).await);
}

#[tokio::test]
async fn two_phase_sparse_runs_phase_b() {
    let tc = TestCluster::from_seed("window", seed_window_set).await;
    let mut client = tc.app().await;
    let (ids, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "sparse".into() }], 5), 20).await;
    assert!(phase_b);
    assert_eq!(ids, reference(&tc, "%sparse%", 5).await);
}

#[tokio::test]
async fn two_phase_pages_no_gap_no_overlap() {
    let tc = TestCluster::from_seed("window", seed_window_set).await;
    let mut client = tc.app().await;
    let mut req = dsl(vec![N::Subject { value: "sparse".into() }], 3);
    let mut all = Vec::new();
    for _ in 0..3 {
        let (ids, next, _) = page_ids(&mut client, &req, 20).await;
        all.extend(ids);
        req.cursor = next;
    }
    assert_eq!(all, reference(&tc, "%sparse%", 9).await);
    let (last, next, _) = page_ids(&mut client, &req, 20).await;
    assert_eq!((last.len(), next), (1, None)); // 10 matches in total
}

#[tokio::test]
async fn null_sent_at_last_and_paged() {
    let tc = TestCluster::from_seed("window", seed_window_set).await;
    let mut client = tc.app().await;
    let expected = reference(&tc, "%dense%", 100).await;
    let mut req = dsl(vec![N::Subject { value: "dense".into() }], 7);
    let mut rows = Vec::new();
    loop {
        let (page, _) = search_page(&mut client, &req, 20).await.unwrap();
        rows.extend(page.rows);
        match page.next_cursor {
            Some(c) => req.cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), expected);
    // the 5 mails without a sent time (sort_at = -infinity) come last, after all 95 dated ones
    assert!(rows[..95].iter().all(|r| r.sent_at.is_some()));
    assert!(rows[95..].iter().all(|r| r.sent_at.is_none()));
}

#[tokio::test]
async fn reset_after_phase_b() {
    let tc = TestCluster::from_seed("window", seed_window_set).await;
    let mut client = tc.app().await;
    let (_, _, phase_b) = page_ids(&mut client, &dsl(vec![N::Subject { value: "sparse".into() }], 5), 20).await;
    assert!(phase_b);
    for setting in ["SHOW enable_indexscan", "SHOW enable_seqscan"] {
        let v: String = client.query_one(setting, &[]).await.unwrap().get(0);
        assert_eq!(v, "on", "{setting}");
    }
}

#[tokio::test]
async fn explain_uses_indexes() {
    let tc = TestCluster::start().await;
    let mails = (1..=300)
        .map(|i| {
            let mut x = m(i, &format!("subject {i}"), &format!("body {i} 報價單 invoice text"), &format!("user{i}@contoso.com"), Some(T0 + i as u64));
            add_recipient(&mut x, RecipientKind::To, None, &format!("r{i}@fabrikam.com"));
            x
        })
        .collect();
    seed_mails(&tc.db, "/p/a.pst", items(mails)).await;
    let a = tc.admin().await;
    a.batch_execute("ANALYZE messages; SET enable_seqscan = off").await.unwrap();
    for (node, index) in [
        (N::Body { value: "報價單".into() }, "messages_body_trgm"),
        (N::Body { value: "invoice".into() }, "messages_body_fts"),
        (N::Sender { value: "contoso".into() }, "messages_sender_trgm"),
        (N::Recipient { value: "fabrikam".into(), kinds: vec![] }, "messages_recipients_trgm"),
    ] {
        let c = compile::compile(&SearchQuery::Dsl { root: ConditionGroup { op: GroupOp::And, children: vec![node] } }).unwrap();
        let params: Vec<_> = c.params.iter().map(|p| p.as_sql()).collect();
        let plan: String = a
            .query(&format!("EXPLAIN SELECT m.id FROM messages m WHERE {}", c.predicate), &params)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plan.contains(index), "{index} missing in\n{plan}");
    }
}

#[tokio::test]
async fn image_returns_bytes_or_none() {
    let tc = TestCluster::start().await;
    seed_mails(&tc.db, "/p/a.pst", items(vec![with_inline_image(m(1, "pic", "x", "s@x.example", Some(T0)), b"RIFF-webp")])).await;
    let blob: i64 = tc.admin().await.query_one("SELECT id FROM image_blobs", &[]).await.unwrap().get(0);
    let svc = SearchService::new(tc.db.clone());
    assert_eq!(svc.image(blob).await.unwrap(), Some(b"RIFF-webp".to_vec()));
    assert_eq!(svc.image(blob + 1000).await.unwrap(), None);
}

#[tokio::test]
async fn filter_options_lists_psts_and_folders() {
    let tc = TestCluster::start().await;
    let a = seed_mails(&tc.db, "/p/b-name.pst", items(vec![m(1, "x", "x", "s@x.example", Some(T0)), m(2, "y", "y", "s@x.example", Some(T0))])).await;
    let b = seed_mails(&tc.db, "/p/a-name.pst", items(vec![m(3, "z", "z", "s@x.example", Some(T0))])).await;
    let f = SearchService::new(tc.db.clone()).filter_options().await.unwrap();
    let psts: Vec<(i64, &str)> = f.psts.iter().map(|p| (p.id, p.display_name.as_str())).collect();
    assert_eq!(psts, vec![(b, "a-name.pst"), (a, "b-name.pst")]);
    let folders: Vec<(i64, &str, i64)> = f.folders.iter().map(|x| (x.pst_id, x.folder_path.as_str(), x.message_count)).collect();
    assert_eq!(folders, vec![(a, "Inbox", 2), (b, "Inbox", 1)]);
}
