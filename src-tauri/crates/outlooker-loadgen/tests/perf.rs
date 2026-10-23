//! Perf gates (spec Part 04 §12.3, Part 02 §9). All `#[ignore]`; run one at a time on the reference machine:
//! `cargo test --workspace --release --test perf -- --ignored --test-threads 1 <name>`; `perf_generate` first.
//! Each gate appends its result to `docs/logs/loadtest-<date>.md` or `docs/logs/perf-<date>.md`, then asserts the
//! threshold. Thresholds are the spec values and are never changed after a run.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::search::{SearchQuery, SearchRequest};
use outlooker_core::importer::remove;
use outlooker_core::search::mail::IMAGE_URL_PREFIX;
use outlooker_core::search::{compile, SearchService};
use outlooker_loadgen::corpus::Corpus;
use outlooker_loadgen::generate::GenConfig;
use outlooker_loadgen::load::{load, load_sources, source_path};
use outlooker_loadgen::rng::Rng;
use outlooker_loadgen::root::LibraryRoot;

#[path = "perf/support.rs"]
mod support;
use support::{canned_queries, explain, generate_json, ms, open_library, percentile, perf_root, record, repo_root, Log};

const MB: f64 = 1024.0 * 1024.0;

fn req(q: &SearchQuery, cursor: Option<String>) -> SearchRequest {
    SearchRequest { query: q.clone(), cursor, page_size: Some(200) }
}

async fn psts(lib: &LibraryRoot) -> Vec<i64> {
    let a = lib.db.connect_admin().await.unwrap();
    a.query("SELECT id FROM pst_sources ORDER BY id", &[]).await.unwrap().iter().map(|r| r.get(0)).collect()
}

/// First page of every canned query: `warmups` untimed runs, then `runs` timed runs each.
async fn first_pages(svc: &SearchService, qs: &[(String, SearchQuery)], warmups: usize, runs: usize) -> (Vec<Duration>, String) {
    let mut samples = Vec::new();
    let mut per_query = String::new();
    for (label, q) in qs {
        for _ in 0..warmups {
            svc.search(req(q, None)).await.unwrap();
        }
        let mut mine = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            svc.search(req(q, None)).await.unwrap_or_else(|e| panic!("{label}: {e:?}"));
            mine.push(t.elapsed());
        }
        per_query.push_str(&format!("{label}: max {}\n", ms(*mine.iter().max().unwrap())));
        samples.extend(mine);
    }
    (samples, per_query)
}

fn read_generate() -> serde_json::Value {
    let text = std::fs::read_to_string(generate_json()).expect("perf-generate.json missing: run perf_generate first");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_generate() {
    let root = perf_root();
    assert!(!root.join("outlooker-data.json").exists(), "perf library already exists at {}: delete it to regenerate", root.display());
    let lib = LibraryRoot::create(&root).await.unwrap();
    let r = load(&lib.db, &GenConfig::full()).await.unwrap();
    let json = serde_json::json!({
        "messages_submitted": r.messages_submitted,
        "duplicates_planned": r.duplicates_planned,
        "messages_stored": r.messages_stored,
        "occurrences": r.occurrences,
        "write_secs": r.write_elapsed.as_secs_f64(),
        "index_secs": r.index_elapsed.as_secs_f64(),
    });
    std::fs::write(generate_json(), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    println!("{json}");
    lib.stop().await.unwrap();
    assert_eq!(r.messages_submitted, 1_000_000);
}

#[test]
#[ignore]
fn perf_writer_throughput() {
    let g = read_generate();
    let rate = g["messages_submitted"].as_f64().unwrap() / g["write_secs"].as_f64().unwrap();
    let pass = rate >= 1_500.0;
    record(Log::Loadtest, "Writer throughput, deferred indexes", ">= 1,500 messages/s", &format!("{rate:.0} messages/s"), pass, &g.to_string());
    assert!(pass, "{rate:.0} messages/s");
}

#[test]
#[ignore]
fn perf_e2e_import() {
    let pst = std::env::var("OUTLOOKER_PERF_PST").expect("set OUTLOOKER_PERF_PST to a large real PST (not in git)");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = std::process::Command::new(cargo)
        .args(["build", "--release", "--workspace", "--example", "import_bench"])
        .current_dir(repo_root().join("src-tauri"))
        .output()
        .expect("build import_bench");
    assert!(built.status.success(), "build import_bench failed: {}", String::from_utf8_lossy(&built.stderr));
    let target = std::env::var_os("CARGO_TARGET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| repo_root().join("src-tauri/target"));
    let out = std::process::Command::new(target.join(format!("release/examples/import_bench{}", std::env::consts::EXE_SUFFIX))).arg(&pst).output().expect("run import_bench");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "import_bench failed: {}", String::from_utf8_lossy(&out.stderr));
    let last = stdout.lines().last().unwrap_or_default();
    let field = |name: &str| -> f64 {
        last.split_whitespace()
            .find_map(|kv| kv.strip_prefix(name).and_then(|v| v.strip_prefix('=')))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("last line lacks {name}=: {last}"))
    };
    let (rate, rss) = (field("messages_per_sec"), field("peak_rss_bytes"));
    let pass = rate >= 150.0 && rss <= 1.5 * 1024.0 * MB;
    let measured = format!("{rate:.0} messages/s, peak RSS {:.0} MB", rss / MB);
    record(Log::Loadtest, "End-to-end import incl. PST read + WebP", ">= 150 messages/s; peak RSS <= 1.5 GB", &measured, pass, &stdout);
    assert!(pass, "{measured}");
}

#[test]
#[ignore]
fn perf_index_rebuild() {
    let g = read_generate();
    let secs = g["index_secs"].as_f64().unwrap();
    let pass = secs <= 30.0 * 60.0;
    record(Log::Loadtest, "Index rebuild (all six) at 1M", "<= 30 min", &format!("{:.1} min", secs / 60.0), pass, "");
    assert!(pass, "{secs:.0} s");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_db_size() {
    let lib = open_library().await;
    let a = lib.db.connect_admin().await.unwrap();
    a.batch_execute("VACUUM (ANALYZE)").await.unwrap();
    let size = |sql: &'static str| {
        let a = &a;
        async move { a.query_one(sql, &[]).await.unwrap().get::<_, i64>(0) as f64 }
    };
    let messages = size("SELECT count(*) FROM messages").await;
    let per_100k = |bytes: f64| bytes / MB * 100_000.0 / messages;
    // (line of Part 04 §6.7, expected MB per 100k, measured bytes)
    let lines = [
        ("messages heap", 80.0, size("SELECT pg_relation_size('messages')").await),
        // One TOAST relation holds all three columns: body_tsv is measured by its stored value sizes and subtracted.
        (
            "messages TOAST (body_html + body_text)",
            700.0,
            size(
                "SELECT pg_total_relation_size(c.reltoastrelid) - (SELECT coalesce(sum(pg_column_size(body_tsv)), 0)::int8 FROM messages) \
                 FROM pg_class c WHERE c.relname = 'messages'",
            )
            .await,
        ),
        ("TOAST body_tsv", 250.0, size("SELECT coalesce(sum(pg_column_size(body_tsv)), 0)::int8 FROM messages").await),
        ("messages_body_trgm", 300.0, size("SELECT pg_relation_size('messages_body_trgm')").await),
        ("messages_body_fts", 150.0, size("SELECT pg_relation_size('messages_body_fts')").await),
        (
            "subject/sender/recipients trigram indexes",
            50.0,
            size("SELECT pg_relation_size('messages_subject_trgm') + pg_relation_size('messages_sender_trgm') + pg_relation_size('messages_recipients_trgm')").await,
        ),
        ("message_recipients + indexes", 80.0, size("SELECT pg_total_relation_size('message_recipients')").await),
        (
            "occurrences, attachments, inline_images + indexes",
            40.0,
            size("SELECT pg_total_relation_size('message_occurrences') + pg_total_relation_size('attachments') + pg_total_relation_size('inline_images')").await,
        ),
        ("image_blobs", 1_500.0, size("SELECT pg_total_relation_size('image_blobs')").await),
    ];
    let mut details = String::new();
    let mut pass = true;
    for (name, expected, bytes) in lines {
        let got = per_100k(bytes);
        let ok = got >= 0.5 * expected && got <= 2.0 * expected;
        pass &= ok;
        details.push_str(&format!("{name}: {got:.0} MB per 100k (expected {expected:.0}) {}\n", if ok { "ok" } else { "OUT OF RANGE" }));
    }
    let total = per_100k(size("SELECT pg_database_size(current_database())").await);
    details.push_str(&format!("total database: {total:.0} MB per 100k (expected 2,900)\n"));
    record(Log::Loadtest, "DB size per 100k", "each line 0.5x-2x of Part 04 §6.7; total reported", &format!("total {total:.0} MB per 100k"), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "{details}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_warm() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, mut details) = first_pages(&svc, &qs, 1, 3).await;
    let mut client = lib.db.connect_app().await.unwrap();
    for (label, q) in &qs {
        details.push_str(&format!("\n== {label}\n{}\n", explain(&mut client, &compile::compile(q).unwrap()).await));
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(500);
    record(Log::Loadtest, "search_mail first page, warm cache (30 canned queries)", "p95 <= 500 ms", &format!("p95 {}", ms(p95)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_cold() {
    let lib = open_library().await;
    lib.stop().await.unwrap(); // server restart: PostgreSQL shared buffers are empty
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, details) = first_pages(&svc, &qs, 0, 1).await;
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_secs(2);
    record(Log::Loadtest, "search_mail first page, cold start (after server restart)", "p95 <= 2 s", &format!("p95 {}", ms(p95)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_short_query() {
    let lib = open_library().await;
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for term in ["報價", "會議", "合約", "客戶", "專案"] {
        let q = SearchQuery::Simple { text: term.into() };
        for _ in 0..3 {
            let t = Instant::now();
            let page = svc.search(req(&q, None)).await.unwrap();
            samples.push(t.elapsed());
            assert!(page.short_query);
        }
    }
    let p95 = percentile(&samples, 0.95);
    let rare = SearchQuery::Simple { text: "qz".into() };
    let t = Instant::now();
    let outcome = svc.search(req(&rare, None)).await;
    let rare_elapsed = t.elapsed();
    let rare_ok = matches!(outcome, Ok(_) | Err(AppError::QueryTimeout { timeout_ms: 20_000 })) && rare_elapsed <= Duration::from_millis(20_500);
    let pool_ok = svc.filter_options().await.is_ok(); // the pool stays healthy after a timeout
    let pass = p95 <= Duration::from_secs(1) && rare_ok && pool_ok;
    let measured = format!("common p95 {}; rare {} ({})", ms(p95), ms(rare_elapsed), if outcome.is_ok() { "results" } else { "queryTimeout" });
    record(Log::Loadtest, "short query (2 chars)", "common p95 <= 1 s; rare: queryTimeout or results within 20 s", &measured, pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "{measured} pool_ok={pool_ok}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_count() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for (label, q) in &qs {
        svc.count(q.clone()).await.unwrap_or_else(|e| panic!("{label}: {e:?}"));
        for _ in 0..3 {
            let t = Instant::now();
            svc.count(q.clone()).await.unwrap();
            samples.push(t.elapsed());
        }
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_secs(2);
    record(Log::Loadtest, "search_count (30 canned queries)", "p95 <= 2 s", &format!("p95 {}", ms(p95)), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_remove_source() {
    let lib = open_library().await;
    let app = lib.db.connect_app().await.unwrap();
    let path = source_path(9);
    let pst_id: i64 = app.query_one("SELECT id FROM pst_sources WHERE path = $1", &[&path]).await.expect("source-09 present").get(0);
    let t = Instant::now();
    let stats = remove::run(&app, pst_id).await.unwrap();
    let elapsed = t.elapsed();
    let pass = elapsed <= Duration::from_secs(300);
    record(Log::Loadtest, "Remove a 100k-message source", "<= 5 min", &format!("{:.1} s", elapsed.as_secs_f64()), pass, &format!("{stats:?}"));
    // restore the library for the next gates (indexes kept: incremental write)
    load_sources(&lib.db, &GenConfig::full(), 9..10, false).await.unwrap();
    lib.stop().await.unwrap();
    assert!(pass, "{elapsed:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_incremental_import() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let done = Arc::new(AtomicBool::new(false));
    let searcher = {
        let (svc, done, qs) = (svc.clone(), done.clone(), qs.clone());
        tokio::spawn(async move {
            let mut samples = Vec::new();
            for (_, q) in qs.iter().cycle() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let t = Instant::now();
                svc.search(req(q, None)).await.unwrap();
                samples.push(t.elapsed());
            }
            samples
        })
    };
    // a 50k-message 11th source; sources 0..=9 keep their 100k items
    let cfg = GenConfig { messages: 550_000, sources: 11, ..GenConfig::full() };
    let report = load_sources(&lib.db, &cfg, 10..11, false).await.unwrap();
    done.store(true, Ordering::Relaxed);
    let samples = searcher.await.unwrap();
    let rate = report.messages_submitted as f64 / report.write_elapsed.as_secs_f64();
    let p95 = percentile(&samples, 0.95);
    let pass = rate >= 300.0 && p95 <= Duration::from_secs(1);
    let measured = format!("{rate:.0} messages/s; searches during import p95 {} ({} samples)", ms(p95), samples.len());
    record(Log::Loadtest, "Incremental import of 50k into a 1M library (indexes kept)", ">= 300 messages/s; searches p95 <= 1 s", &measured, pass, "");
    // back to the 1M library
    let app = lib.db.connect_app().await.unwrap();
    let path = source_path(10);
    let id: i64 = app.query_one("SELECT id FROM pst_sources WHERE path = $1", &[&path]).await.unwrap().get(0);
    remove::run(&app, id).await.unwrap();
    lib.stop().await.unwrap();
    assert!(pass, "{measured}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_next_page() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let mut samples = Vec::new();
    for (_, q) in &qs {
        let mut cursor = svc.search(req(q, None)).await.unwrap().next_cursor;
        for _ in 2..=5 {
            let Some(c) = cursor.take() else { break };
            let t = Instant::now();
            cursor = svc.search(req(q, Some(c))).await.unwrap().next_cursor;
            samples.push(t.elapsed());
        }
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(100);
    record(Log::Perf, "search_mail next page (pages 2-5)", "p95 <= 100 ms", &format!("p95 {} ({} samples)", ms(p95), samples.len()), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_mail_get_first_image() {
    let lib = open_library().await;
    let a = lib.db.connect_admin().await.unwrap();
    let ids: Vec<i64> = a.query("SELECT id FROM messages", &[]).await.unwrap().iter().map(|r| r.get(0)).collect();
    let svc = SearchService::new(lib.db.clone());
    let mut rng = Rng::new(99);
    let mut samples = Vec::new();
    let mut with_image = 0;
    for _ in 0..1_000 {
        let id = ids[rng.below(ids.len() as u64) as usize];
        let t = Instant::now();
        let mail = svc.mail_get(id).await.unwrap();
        let first = mail.body_html.as_deref().and_then(|h| h.split(IMAGE_URL_PREFIX).nth(1)).and_then(|s| s.split('"').next()).and_then(|s| s.parse::<i64>().ok());
        if let Some(blob) = first {
            assert!(svc.image(blob).await.unwrap().is_some());
            with_image += 1;
        }
        samples.push(t.elapsed());
    }
    let p95 = percentile(&samples, 0.95);
    let pass = p95 <= Duration::from_millis(100);
    record(Log::Perf, "mail_get + first mailimg image (1,000 random ids)", "p95 <= 100 ms", &format!("p95 {} ({with_image} with an image)", ms(p95)), pass, "");
    lib.stop().await.unwrap();
    assert!(pass, "p95 {}", ms(p95));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn perf_search_first_page_p50() {
    let lib = open_library().await;
    let qs = canned_queries(&Corpus::new(), &psts(&lib).await);
    let svc = SearchService::new(lib.db.clone());
    let (samples, details) = first_pages(&svc, &qs, 1, 3).await;
    let p50 = percentile(&samples, 0.50);
    let pass = p50 <= Duration::from_millis(150);
    record(Log::Perf, "search_mail first page p50 (30 canned queries, warm)", "p50 <= 150 ms", &format!("p50 {}", ms(p50)), pass, &details);
    lib.stop().await.unwrap();
    assert!(pass, "p50 {}", ms(p50));
}
