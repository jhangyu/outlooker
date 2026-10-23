#![cfg(feature = "test-support")]
//! App composition tests on a real PostgreSQL (run `node scripts/fetch-postgres.mjs` first).
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use outlooker_core::app::{self, AppDirs, BuildOptions, Services, ShutdownLimits, ShutdownReport, StartStep};
use outlooker_core::db::indexes::missing_or_invalid;
use outlooker_core::db::Db;
use outlooker_core::dto::app::Language;
use outlooker_core::dto::import::{EnqueueResult, ImportPhase, ImportStatus};
use outlooker_core::importer::{ImportTuning, RecoveryReport};
use outlooker_core::lifecycle::SupervisorConfig;
use outlooker_core::paths::dev_pg_dir;
use outlooker_core::sink::{CoreEvent, EventSink, RecordingSink};
use outlooker_core::test_support::import_fixtures::new_source;
use outlooker_core::test_support::TestCluster;
use tokio::sync::mpsc;
use tokio::time::timeout;

const LONG: Duration = Duration::from_secs(120);
const STEPS: [StartStep; 3] = [StartStep::Recovered, StartStep::IndexesEnsured, StartStep::WorkerStarted];

/// Records every event (RecordingSink), forwards it to a channel for event waits, and for one watched PST records,
/// at the moment each `import:status-changed` (status other than `queued`) is emitted, whether all six search
/// indexes were valid. `queued` is excluded: `recover_after_crash` may report it before the heal.
struct TestSink {
    rec: RecordingSink,
    tx: mpsc::UnboundedSender<CoreEvent>,
    watched: Mutex<Option<(i64, Db)>>,
    probes: Mutex<Vec<(ImportStatus, bool)>>,
}

impl TestSink {
    fn new() -> (Arc<TestSink>, mpsc::UnboundedReceiver<CoreEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sink = TestSink { rec: RecordingSink::default(), tx, watched: Mutex::new(None), probes: Mutex::new(Vec::new()) };
        (Arc::new(sink), rx)
    }

    fn watch(&self, pst_id: i64, db: Db) {
        *self.watched.lock().unwrap() = Some((pst_id, db));
    }

    fn probes(&self) -> Vec<(ImportStatus, bool)> {
        self.probes.lock().unwrap().clone()
    }
}

impl EventSink for TestSink {
    fn emit(&self, e: CoreEvent) {
        if let CoreEvent::ImportStatusChanged(c) = &e {
            let watched = self.watched.lock().unwrap().clone();
            if let Some((pst_id, db)) = watched
                && c.pst_id == pst_id
                && c.status != ImportStatus::Queued
            {
                // multi_thread test runtime: block this worker thread for one catalog query
                let valid = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let admin = db.connect_admin().await.expect("admin connection");
                        missing_or_invalid(&admin).await.expect("index check").is_empty()
                    })
                });
                self.probes.lock().unwrap().push((c.status, valid));
            }
        }
        self.rec.emit(e.clone());
        let _ = self.tx.send(e);
    }
}

fn dirs(root: &Path) -> AppDirs {
    assert!(dev_pg_dir().join("bin").exists(), "PostgreSQL binaries missing: run node scripts/fetch-postgres.mjs");
    AppDirs { app_local_data_dir: root.join("app"), app_log_dir: root.join("logs"), pg_dir: dev_pg_dir() }
}

fn data_root(root: &Path) -> PathBuf {
    root.join("data")
}

fn opts(import_tuning: ImportTuning) -> BuildOptions {
    let supervisor = SupervisorConfig { probe_interval: Duration::from_millis(200), ..SupervisorConfig::default() };
    BuildOptions { supervisor, import_tuning }
}

/// Services wired onto the test's database of the shared cluster (the Supervisor never starts or stops PostgreSQL):
/// the data root carries the marker and the bootstrap settings point at it.
fn app_on(tc: &TestCluster, sink: Arc<dyn EventSink>, tuning: ImportTuning) -> Services {
    std::fs::write(tc.paths.data_root().join("outlooker-data.json"), b"{}").unwrap();
    let s = app::build(dirs(tc.dir.path()), sink, opts(tuning)).unwrap();
    s.settings.set_bootstrap(Some(data_root(tc.dir.path()).display().to_string()), Language::En).unwrap();
    s.supervisor.attach_test_db(tc.db_conn());
    s
}

async fn job_state(admin: &tokio_postgres::Client, pst_id: i64) -> String {
    admin
        .query_one("SELECT state FROM import_jobs WHERE pst_id = $1 ORDER BY id DESC LIMIT 1", &[&pst_id])
        .await
        .unwrap()
        .get(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_during_import_requeues_job() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/external/outlook.pst");
    assert!(fixture.exists(), "missing fixture {}", fixture.display());
    let tc = TestCluster::start().await;
    let (sink, mut events) = TestSink::new();
    let tuning = ImportTuning { hold_reader_after: Some(0), ..ImportTuning::default() };
    let s = app_on(&tc, sink, tuning);
    let report = timeout(LONG, app::start_services(&s)).await.expect("start timeout").unwrap();
    assert_eq!(report.steps, STEPS);
    let mut job_events = s.importer.job_events();
    let path = fixture.canonicalize().unwrap().display().to_string();
    let pst_id = match &s.importer.enqueue(vec![path]).await.unwrap()[..] {
        [EnqueueResult::Queued { pst }] => pst.id,
        other => panic!("enqueue: {other:?}"),
    };
    timeout(LONG, async {
        loop {
            match events.recv().await.expect("sink closed") {
                CoreEvent::ImportProgress(p) if p.pst_id == pst_id && p.phase == ImportPhase::Importing => break,
                _ => {}
            }
        }
    })
    .await
    .expect("the job never reached importing");

    let started = Instant::now();
    let report = app::shutdown(&s, ShutdownLimits::default()).await;
    let elapsed = started.elapsed();
    assert_eq!(report, ShutdownReport { import_drained: true, db_stopped: true });
    assert!(elapsed < Duration::from_secs(45), "shutdown took {elapsed:?} (budget 15 s + 30 s)");
    assert_eq!(job_events.try_recv().unwrap(), (pst_id, ImportStatus::Queued), "Queued, not Cancelled");

    // The job row and its checkpoint were committed (the real PostgreSQL stop is covered by the cluster_lifecycle tests).
    let admin = tc.admin().await;
    assert_eq!(job_state(&admin, pst_id).await, "queued");
    let checkpoints: i64 = admin
        .query_one("SELECT count(*) FROM import_checkpoints WHERE pst_id = $1", &[&pst_id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(checkpoints, 1, "checkpoint row committed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn start_after_kill_recovers_jobs_then_heals_then_starts_worker() {
    let tc = TestCluster::start().await;
    let admin = tc.admin().await;
    // The requeued job points at a missing file, so the worker's first event for it is `failed` (no index drop).
    let gone = tc.dir.path().join("gone");
    // A real kill leaves at most one active job; the test needs two, so lift the one-active-job index for this setup.
    admin.batch_execute("DROP INDEX import_jobs_one_running").await.unwrap();
    let (running_pst, _) = new_source(&admin, &gone.join("running.pst").display().to_string()).await;
    let (cancelling_pst, cancelling_job) = new_source(&admin, &gone.join("cancelling.pst").display().to_string()).await;
    admin.execute("UPDATE import_jobs SET state = 'cancelling' WHERE id = $1", &[&cancelling_job]).await.unwrap();
    admin.batch_execute("DROP INDEX messages_body_trgm").await.unwrap();
    assert_eq!(missing_or_invalid(&admin).await.unwrap(), vec!["messages_body_trgm"]);
    drop(admin);

    let (sink, mut events) = TestSink::new();
    let s2 = app_on(&tc, sink.clone(), ImportTuning::default());
    sink.watch(running_pst, s2.db.clone());
    let report = timeout(LONG, app::start_services(&s2)).await.expect("start timeout").unwrap();
    assert_eq!(report.steps, STEPS);
    assert_eq!(report.recovery, RecoveryReport { requeued: vec![running_pst], cancelled: vec![cancelling_pst] });

    let ended = timeout(LONG, async {
        loop {
            match events.recv().await.expect("sink closed") {
                CoreEvent::ImportStatusChanged(c) if c.pst_id == running_pst && c.status == ImportStatus::Failed => break c.status,
                _ => {}
            }
        }
    })
    .await
    .expect("the requeued job never ran");
    assert_eq!(ended, ImportStatus::Failed);
    let probes = sink.probes();
    assert!(!probes.is_empty(), "no worker event for the requeued PST; events: {:?}", sink.rec.names());
    assert!(probes.iter().all(|(_, valid)| *valid), "worker touched the job before the indexes were healed: {probes:?}");

    let admin = tc.admin().await;
    assert_eq!(job_state(&admin, cancelling_pst).await, "cancelled");
    assert!(missing_or_invalid(&admin).await.unwrap().is_empty());
    let valid: bool = admin
        .query_one(
            "SELECT i.indisvalid FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid WHERE c.relname = 'messages_body_trgm'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid, "the dropped index was rebuilt");
    drop(admin);
    app::shutdown(&s2, ShutdownLimits::default()).await;
}
