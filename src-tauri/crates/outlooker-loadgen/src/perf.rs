//! Helpers of the `perf_` gate tests (spec Part 04 §12.3, Part 02 §9): library location, result log, canned
//! queries, percentiles, EXPLAIN capture.
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use outlooker_core::dto::search::{ConditionGroup, ConditionNode as N, GroupOp, RecipientKind, SearchQuery};
use outlooker_core::importer::dedupe::format_utc_seconds;
use outlooker_core::search::compile::Compiled;
use outlooker_core::search::execute::{bind, ordered_sql, phase_a_sql, WINDOW};

use crate::corpus::Corpus;
use crate::root::LibraryRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Log {
    /// `docs/logs/loadtest-<date>.md` (Part 04 §12.3 rows)
    Loadtest,
    /// `docs/logs/perf-<date>.md` (Part 02 §9 rows)
    Perf,
}

/// Repository root (this crate is `src-tauri/crates/outlooker-loadgen`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// `$OUTLOOKER_PERF_ROOT`, default `src-tauri/target/perf-root`.
pub fn perf_root() -> PathBuf {
    std::env::var_os("OUTLOOKER_PERF_ROOT").map(PathBuf::from).unwrap_or_else(|| repo_root().join("src-tauri/target/perf-root"))
}

pub fn generate_json() -> PathBuf {
    perf_root().join("perf-generate.json")
}

pub async fn open_library() -> LibraryRoot {
    let root = perf_root();
    assert!(
        root.join("outlooker-data.json").exists(),
        "perf library missing at {}: run `cargo test -p outlooker-loadgen --release -- --ignored perf_generate` first",
        root.display()
    );
    LibraryRoot::open(&root).await.expect("open perf library")
}

pub fn commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Appends one result (a one-row table plus optional details) to the log of today (UTC) and returns its path.
pub fn record(log: Log, gate: &str, threshold: &str, measured: &str, pass: bool, details: &str) -> PathBuf {
    let date = &format_utc_seconds(SystemTime::now())[..10];
    let name = match log {
        Log::Loadtest => format!("loadtest-{date}.md"),
        Log::Perf => format!("perf-{date}.md"),
    };
    let path = repo_root().join("docs/logs").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).expect("docs/logs");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).expect("open log");
    let result = if pass { "PASS" } else { "FAIL" };
    let mut text = format!(
        "\n## {gate}\n\n| gate | threshold | measured | result | commit |\n|---|---|---|---|---|\n| {gate} | {threshold} | {measured} | {result} | {} |\n",
        commit()
    );
    if !details.is_empty() {
        text.push_str(&format!("\n```text\n{}\n```\n", details.trim_end()));
    }
    f.write_all(text.as_bytes()).expect("write log");
    path
}

/// Nearest-rank percentile (`p` in 0..=1) of `samples`.
pub fn percentile(samples: &[Duration], p: f64) -> Duration {
    assert!(!samples.is_empty(), "no samples");
    let mut s = samples.to_vec();
    s.sort();
    let rank = ((p * s.len() as f64).ceil() as usize).clamp(1, s.len());
    s[rank - 1]
}

pub fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn dsl(op: GroupOp, children: Vec<N>) -> SearchQuery {
    SearchQuery::Dsl { root: ConditionGroup { op, children } }
}

/// The 30 canned queries of spec Part 04 §12.3 (every text value ≥ 3 characters): 6 sender, 6 recipient,
/// 6 English body (common and rare by Zipf rank), 6 Chinese body, 6 DSL combinations. `psts` = the library's PST ids.
pub fn canned_queries(c: &Corpus, psts: &[i64]) -> Vec<(String, SearchQuery)> {
    let body = |v: &str| dsl(GroupOp::And, vec![N::Body { value: v.into() }]);
    let mut q: Vec<(String, SearchQuery)> = Vec::new();
    for v in ["loadgen.example", "sender12", "sender19999@", "Sender 7", "sender42@loadgen", "sender305"] {
        q.push((format!("sender {v}"), dsl(GroupOp::And, vec![N::Sender { value: v.into() }])));
    }
    for (v, kinds) in [
        ("corp7.example", vec![]),
        ("user123", vec![]),
        ("user49999@", vec![]),
        ("corp1", vec![RecipientKind::Cc]),
        ("User 77", vec![]),
        ("user2024", vec![RecipientKind::To, RecipientKind::Bcc]),
    ] {
        q.push((format!("recipient {v} {kinds:?}"), dsl(GroupOp::And, vec![N::Recipient { value: v.into(), kinds }])));
    }
    for rank in [0, 1, 5, 50, 500, 1049] {
        q.push((format!("body en rank {rank} {}", c.en[rank]), body(c.en[rank])));
    }
    for v in ["報價單", "資料庫", "電話會議", "分公司", "董事會", "會議室"] {
        q.push((format!("body zh {v}"), body(v)));
    }
    let src = psts.first().copied().unwrap_or(1);
    q.push((
        "dsl sender + 2020".into(),
        dsl(GroupOp::And, vec![N::Sender { value: "sender1".into() }, N::DateRange { from: Some("2020-01-01T00:00:00Z".into()), to: Some("2021-01-01T00:00:00Z".into()) }]),
    ));
    q.push(("dsl body en + attachment".into(), dsl(GroupOp::And, vec![N::Body { value: c.en[2].into() }, N::HasAttachment { value: true }])));
    q.push(("dsl recipient + source".into(), dsl(GroupOp::And, vec![N::Recipient { value: "corp3".into(), kinds: vec![] }, N::SourcePst { pst_ids: vec![src] }])));
    q.push((
        "dsl body zh + folder".into(),
        dsl(GroupOp::And, vec![N::Body { value: "報價單".into() }, N::Folder { folder_path: "Inbox/2020".into(), include_subfolders: false }]),
    ));
    q.push(("dsl or(body en, subject zh)".into(), dsl(GroupOp::Or, vec![N::Body { value: c.en[10].into() }, N::Subject { value: "會議室".into() }])));
    q.push((format!("simple {} 報價單", c.en[3]), SearchQuery::Simple { text: format!("{} 報價單", c.en[3]) }));
    assert_eq!(q.len(), 30);
    q
}

/// `EXPLAIN (ANALYZE, BUFFERS)` of phase A and of phase B (planner settings of §7.4) for a first page of 200.
pub async fn explain(client: &mut tokio_postgres::Client, c: &Compiled) -> String {
    let (cs, ci, n) = ("infinity".to_owned(), 0_i64, 201_i64);
    let tx = client.transaction().await.expect("tx");
    let lines = |rows: Vec<tokio_postgres::Row>| rows.iter().map(|r| r.get::<_, String>(0)).collect::<Vec<_>>().join("\n");
    let a = tx.query(&format!("EXPLAIN (ANALYZE, BUFFERS) {}", phase_a_sql(c)), &bind(c, &[&cs, &ci, &WINDOW, &n])).await.expect("explain A");
    tx.batch_execute("SET LOCAL enable_indexscan = off; SET LOCAL enable_seqscan = off;").await.expect("set");
    let b = tx.query(&format!("EXPLAIN (ANALYZE, BUFFERS) {}", ordered_sql(c)), &bind(c, &[&cs, &ci, &n])).await.expect("explain B");
    tx.rollback().await.expect("rollback");
    format!("-- phase A\n{}\n-- phase B\n{}", lines(a), lines(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank_and_canned_set() {
        let s: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&s, 0.95), Duration::from_millis(95));
        assert_eq!(percentile(&s, 0.50), Duration::from_millis(50));
        assert_eq!(percentile(&[Duration::from_millis(7)], 0.95), Duration::from_millis(7));
        let qs = canned_queries(&Corpus::new(), &[1, 2]);
        for (label, q) in &qs {
            let c = outlooker_core::search::compile::compile(q).unwrap_or_else(|e| panic!("{label}: {e:?}"));
            assert!(c.indexable && !c.short_query, "{label} must be an indexable term of ≥ 3 characters");
        }
    }
}
