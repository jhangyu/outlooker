//! Import throughput bench (spec Part 02 §9): imports a given PST into a fresh cluster and prints
//! messages/s and MB/s per phase. The last stdout line is exactly
//! `import_bench messages_per_sec=<f64> peak_rss_bytes=<u64>` (messages/s = items done / import-phase seconds;
//! peak RSS of this process, sampled at every progress event). Run from src-tauri/: `cargo build --workspace --release --example import_bench`,
//! then `target/release/examples/import_bench <absolute path to .pst>` (outlooker-loadgen turns test-support on for the workspace).

#[cfg(not(feature = "test-support"))]
fn main() {
    eprintln!("import_bench runs on a TestCluster: build with cargo build --workspace");
    std::process::exit(2);
}

#[cfg(feature = "test-support")]
#[tokio::main]
async fn main() {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use outlooker_core::dto::app::AppPhase;
    use outlooker_core::dto::import::{EnqueueResult, ImportPhase, ImportStatus};
    use outlooker_core::gate::Gate;
    use outlooker_core::importer::{ImportManager, ImportTuning};
    use outlooker_core::sink::{CoreEvent, EventSink};
    use outlooker_core::test_support::TestCluster;

    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    /// First time each phase is seen, the last progress counters, and the bench process RSS sampled at every
    /// progress event (one per committed batch and phase change; no timer).
    struct PhaseClock {
        seen: Mutex<Vec<(ImportPhase, Instant)>>,
        last: Mutex<Option<(i64, i64)>>, // (items_done, stored)
        rss: Mutex<(System, Pid, u64)>,  // (sampler, own pid, peak bytes)
    }
    impl PhaseClock {
        fn sample_rss(&self) {
            let mut guard = self.rss.lock().unwrap();
            let (sys, pid, peak) = &mut *guard;
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(std::slice::from_ref(pid)), false, ProcessRefreshKind::nothing().with_memory());
            if let Some(p) = sys.process(*pid) {
                *peak = (*peak).max(p.memory());
            }
        }
    }
    impl EventSink for PhaseClock {
        fn emit(&self, event: CoreEvent) {
            if let CoreEvent::ImportProgress(p) = event {
                let mut seen = self.seen.lock().unwrap();
                if !seen.iter().any(|(ph, _)| *ph == p.phase) {
                    seen.push((p.phase, Instant::now()));
                }
                *self.last.lock().unwrap() = Some((p.items_done, p.stored));
                drop(seen);
                self.sample_rss();
            }
        }
    }

    let path = std::env::args().nth(1).expect("usage: import_bench <absolute path to .pst>");
    let size_mb = std::fs::metadata(&path).expect("PST file").len() as f64 / (1024.0 * 1024.0);
    let tc = TestCluster::start().await;
    let pid = sysinfo::get_current_pid().expect("own pid");
    let clock = Arc::new(PhaseClock { seen: Mutex::new(Vec::new()), last: Mutex::new(None), rss: Mutex::new((System::new(), pid, 0)) });
    clock.sample_rss();
    let (_phase, rx) = tokio::sync::watch::channel(AppPhase::Ready);
    let mgr = ImportManager::new(tc.db.clone(), Gate::new(), tc.paths.clone(), clock.clone(), rx, ImportTuning::default());
    let mut events = mgr.job_events();
    let start = Instant::now();
    let res = mgr.register(vec![path.clone()]).await.expect("register");
    let EnqueueResult::Queued { pst } = &res[0] else { panic!("rejected: {res:?}") };
    mgr.start_worker();
    let status = loop {
        let (id, status) = events.recv().await.expect("job events");
        if id == pst.id {
            break status;
        }
    };
    let end = Instant::now();
    assert_eq!(status, ImportStatus::Completed, "job ended {status:?}");
    let seen = clock.seen.lock().unwrap().clone();
    let at = |ph: ImportPhase| seen.iter().find(|(p, _)| *p == ph).map(|(_, t)| *t);
    let importing = at(ImportPhase::Importing).unwrap_or(start);
    let finalizing = at(ImportPhase::Finalizing).unwrap_or(end);
    let (items, stored) = clock.last.lock().unwrap().unwrap_or((0, 0));
    let secs = |a: Instant, b: Instant| b.duration_since(a).as_secs_f64().max(1e-9);
    println!("file            {path} ({size_mb:.1} MB)");
    println!("scan            {:.2} s", secs(start, importing));
    println!(
        "import          {:.2} s  {:.1} messages/s  {:.2} MB/s  ({items} items, {stored} stored)",
        secs(importing, finalizing),
        items as f64 / secs(importing, finalizing),
        size_mb / secs(importing, finalizing)
    );
    println!("finalize        {:.2} s  (indexes, VACUUM)", secs(finalizing, end));
    println!("total           {:.2} s  {:.2} MB/s", secs(start, end), size_mb / secs(start, end));
    clock.sample_rss();
    let peak_rss_bytes = clock.rss.lock().unwrap().2;
    // last line: machine-read by the perf test; keep the exact format
    println!("import_bench messages_per_sec={} peak_rss_bytes={peak_rss_bytes}", items as f64 / secs(importing, finalizing));
}
