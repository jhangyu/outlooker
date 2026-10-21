//! R2-12 SQL Console integration tests (spec Part 02 §10 bypass list, Part 04 §8.1, §12.2 row Read-only role).
#![cfg(feature = "test-support")]
use std::time::{Duration, Instant};

use outlooker_core::dto::error::AppError;
use outlooker_core::dto::sql::{SavedQueryInput, SqlExecuteRequest, SqlResult};
use outlooker_core::sql_console::SqlConsole;
use outlooker_core::test_support::TestCluster;

fn req(id: &str, sql: &str, max_rows: Option<u32>) -> SqlExecuteRequest {
    SqlExecuteRequest { execution_id: id.to_owned(), sql: sql.to_owned(), max_rows }
}

/// One source row and two messages, written by the table owner.
async fn seed(tc: &TestCluster) {
    tc.admin()
        .await
        .batch_execute(
            "INSERT INTO pst_sources (path, display_name, file_size_bytes, file_mtime) VALUES ('/seed.pst', 'seed', 1, now());
             INSERT INTO messages (dedupe_key, dedupe_kind, body_source, subject, body_text) VALUES
               (decode(repeat('01', 32), 'hex'), 'content_hash', 'text', 'first', 'body one'),
               (decode(repeat('02', 32), 'hex'), 'content_hash', 'text', 'second', 'body two');",
        )
        .await
        .unwrap();
}

async fn pst_count(tc: &TestCluster) -> i64 {
    tc.admin().await.query_one("SELECT count(*) FROM pst_sources", &[]).await.unwrap().get(0)
}

#[tokio::test]
async fn bypass_attempts_all_fail() {
    let tc = TestCluster::start().await;
    seed(&tc).await;
    let sc = SqlConsole::new(tc.db.clone());
    let before = pst_count(&tc).await;
    let attempts = [
        "INSERT INTO v_pst_sources (path) VALUES ('x')",
        "UPDATE v_pst_sources SET path = 'x'",
        "DELETE FROM v_pst_sources",
        "SET TRANSACTION READ WRITE; DELETE FROM v_pst_sources",
        "SET ROLE outlooker_app",
        "CREATE TEMP TABLE t (x int)",
        "COPY (SELECT 1) TO 'copy-probe.txt'",
        "SELECT pg_read_file('/etc/hosts')",
        "DO $$BEGIN DELETE FROM v_pst_sources; END$$",
        "SELECT nextval('messages_id_seq')",
        "CREATE FUNCTION f() RETURNS int LANGUAGE sql AS 'SELECT 1'",
        "ALTER ROLE outlooker_ro SET work_mem = '1GB'",
    ];
    for sql in attempts {
        let r = sc.execute(req("e1", sql, None)).await;
        assert!(r.is_err(), "{sql} must fail, got {r:?}");
    }
    // Accepted by PostgreSQL inside BEGIN READ ONLY (measured), but the transaction is rolled back per execution.
    let r = sc.execute(req("e2", "SET TRANSACTION READ WRITE", None)).await.unwrap();
    assert!(matches!(r, SqlResult::Command { ref tag, .. } if tag == "SET"), "{r:?}");
    assert!(sc.execute(req("e3", "DELETE FROM v_pst_sources", None)).await.is_err());
    assert_eq!(pst_count(&tc).await, before);
    assert_eq!(before, 1);
}

#[tokio::test]
async fn read_only_role_rows() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    assert_eq!(sc.execute(req("r1", "DELETE FROM v_mail", None)).await, Err(AppError::ReadOnlyViolation));
    match sc.execute(req("r2", "SELECT * FROM messages", None)).await {
        Err(AppError::SqlError { sqlstate, .. }) => assert_eq!(sqlstate, "42501"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        sc.execute(req("r3", "SET statement_timeout=0; SELECT 1", None)).await,
        Err(AppError::invalid("sql", "multipleStatements"))
    );
    let t = Instant::now();
    assert_eq!(sc.execute(req("r4", "SELECT pg_sleep(40)", None)).await, Err(AppError::QueryTimeout { timeout_ms: 30_000 }));
    assert!(t.elapsed() < Duration::from_secs(39), "statement_timeout 30 s must fire, took {:?}", t.elapsed());
    // syntax errors keep their 1-based position for the editor marker
    match sc.execute(req("r5", "SELECT nosuchcolumn FROM v_mail", None)).await {
        Err(AppError::SqlError { sqlstate, position, .. }) => assert_eq!((sqlstate.as_str(), position), ("42703", Some(8))),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn cancel_during_sleep() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let runner = sc.clone();
    let t = Instant::now();
    let mut task = tokio::spawn(async move { runner.execute(req("c1", "SELECT pg_sleep(10)", None)).await });
    tokio::select! {
        _ = sc.wait_statement_started() => {}
        r = &mut task => panic!("execute ended before its statement started: {r:?}"),
    }
    sc.cancel("c1".to_owned()).await.unwrap();
    assert_eq!(task.await.unwrap(), Err(AppError::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(9), "cancel must interrupt pg_sleep, took {:?}", t.elapsed());
    assert_eq!(sc.cancel("unknown".to_owned()).await, Ok(()));
    assert_eq!(sc.cancel("c1".to_owned()).await, Ok(())); // finished id: idempotent
    // the connection went back to the pool clean: the next execution works
    assert!(matches!(sc.execute(req("c2", "SELECT 1 AS x", None)).await, Ok(SqlResult::Table { .. })));
}

#[tokio::test]
async fn mode_detection() {
    let tc = TestCluster::start().await;
    seed(&tc).await;
    let sc = SqlConsole::new(tc.db.clone());
    match sc.execute(req("m1", "SELECT id FROM v_mail ORDER BY id DESC", None)).await.unwrap() {
        SqlResult::MailList { rows, missing_ids, truncated, .. } => {
            assert_eq!(rows.iter().map(|r| r.subject.as_str()).collect::<Vec<_>>(), ["second", "first"]);
            assert_eq!((missing_ids, truncated), (0, false));
        }
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m2", "SELECT 999999::int8 AS id UNION ALL SELECT min(id) FROM v_mail", None)).await.unwrap() {
        SqlResult::MailList { rows, missing_ids, .. } => assert_eq!((rows.len(), missing_ids), (1, 1)),
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m3", "SELECT 1 AS x", None)).await.unwrap() {
        SqlResult::Table { columns, rows, truncated, .. } => {
            assert_eq!((columns[0].name.as_str(), columns[0].pg_type.as_str()), ("x", "int4"));
            assert_eq!(rows, vec![vec![Some("1".to_owned())]]);
            assert!(!truncated);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        sc.execute(req("m4", "SET work_mem = '8MB'", None)).await.unwrap(),
        SqlResult::Command { ref tag, .. } if tag == "SET"
    ));
    match sc.execute(req("m5", "SHOW work_mem", None)).await.unwrap() {
        SqlResult::Table { rows, .. } => assert_eq!(rows, vec![vec![Some("64MB".to_owned())]]), // role default, the SET above was rolled back
        other => panic!("{other:?}"),
    }
    match sc.execute(req("m6", "SELECT repeat('x', 2500) AS long, NULL::text AS n", None)).await.unwrap() {
        SqlResult::Table { rows, .. } => {
            assert_eq!(rows[0][0].as_ref().unwrap().chars().count(), 2_001);
            assert_eq!(rows[0][1], None);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn max_rows_limits() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    assert!(matches!(
        sc.execute(req("x1", "SELECT 1", Some(10_001))).await,
        Err(AppError::InvalidInput { ref field, ref reason }) if field == "maxRows" && reason == "outOfRange"
    ));
    assert_eq!(sc.execute(req("x2", "SELECT 1", Some(0))).await, Err(AppError::invalid("maxRows", "outOfRange")));
    assert_eq!(sc.execute(req("x3", "   ", None)).await, Err(AppError::invalid("sql", "empty")));
    assert_eq!(sc.execute(req("", "SELECT 1", None)).await, Err(AppError::invalid("executionId", "empty")));
    let table = |r: SqlResult| match r {
        SqlResult::Table { rows, truncated, .. } => (rows.len(), truncated),
        other => panic!("{other:?}"),
    };
    assert_eq!(table(sc.execute(req("x4", "SELECT g FROM generate_series(1, 1500) g", None)).await.unwrap()), (1_000, true));
    assert_eq!(table(sc.execute(req("x5", "SELECT g FROM generate_series(1, 5) g", Some(5))).await.unwrap()), (5, false));
    assert_eq!(table(sc.execute(req("x6", "SELECT g FROM generate_series(1, 6) g", Some(5))).await.unwrap()), (5, true));
    assert_eq!(table(sc.execute(req("x7", "SELECT g FROM generate_series(1, 10000) g", Some(10_000))).await.unwrap()), (10_000, false));
}

#[tokio::test]
async fn one_execution_at_a_time() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let runner = sc.clone();
    let mut task = tokio::spawn(async move { runner.execute(req("a1", "SELECT pg_sleep(2)", None)).await });
    tokio::select! {
        _ = sc.wait_statement_started() => {}
        r = &mut task => panic!("execute ended before its statement started: {r:?}"),
    }
    assert_eq!(sc.execute(req("a2", "SELECT 1", None)).await, Err(AppError::invalid("executionId", "alreadyRunning")));
    assert!(matches!(task.await.unwrap(), Ok(SqlResult::Table { .. })));
    // the registration is removed after the first finished
    assert!(sc.execute(req("a3", "SELECT 1", None)).await.is_ok());
}

#[tokio::test]
async fn saved_queries_crud() {
    let tc = TestCluster::start().await;
    let sc = SqlConsole::new(tc.db.clone());
    let input = |name: &str, sql: &str| SavedQueryInput { name: name.to_owned(), sql: sql.to_owned() };
    let b = sc.saved_create(input("  beta  ", "SELECT 2")).await.unwrap();
    assert_eq!(b.name, "beta");
    assert_eq!(b.updated_at.len(), 20, "RFC 3339 UTC: {}", b.updated_at);
    assert!(b.updated_at.ends_with('Z'));
    let a = sc.saved_create(input("alpha", "SELECT 1")).await.unwrap();
    assert_eq!(sc.saved_create(input("alpha", "SELECT 3")).await, Err(AppError::invalid("name", "duplicate")));
    assert_eq!(sc.saved_create(input("   ", "SELECT 3")).await, Err(AppError::invalid("name", "empty")));
    assert_eq!(sc.saved_create(input(&"n".repeat(201), "SELECT 3")).await, Err(AppError::invalid("name", "tooLong")));
    assert_eq!(sc.saved_create(input("ok", " \n ")).await, Err(AppError::invalid("sql", "empty")));
    assert_eq!(sc.saved_create(input("ok", &"x".repeat(100_001))).await, Err(AppError::invalid("sql", "tooLong")));
    assert!(sc.saved_create(input(&"報".repeat(200), &"x".repeat(100_000))).await.is_ok());
    let updated = sc.saved_update(b.id, input("gamma", "SELECT 22")).await.unwrap();
    assert_eq!((updated.id, updated.name.as_str(), updated.sql.as_str()), (b.id, "gamma", "SELECT 22"));
    assert_eq!(sc.saved_update(b.id, input("alpha", "SELECT 9")).await, Err(AppError::invalid("name", "duplicate")));
    let names: Vec<String> = sc.saved_list().await.unwrap().into_iter().map(|q| q.name).collect();
    assert_eq!(names[..2], ["alpha".to_owned(), "gamma".to_owned()]);
    sc.saved_delete(a.id).await.unwrap();
    assert_eq!(sc.saved_delete(a.id).await, Err(AppError::not_found("savedQuery", a.id)));
    assert_eq!(sc.saved_update(a.id, input("zeta", "SELECT 1")).await, Err(AppError::not_found("savedQuery", a.id)));
    assert_eq!(sc.saved_list().await.unwrap().len(), 2);
}
