//! Pure helpers of the SQL Console Service (spec Part 04 §8.1): error mapping, cell cut, command tag.
use tokio_postgres::error::ErrorPosition;

use crate::dto::error::AppError;

use super::{CELL_MAX_CHARS, TIMEOUT_MS};

const MULTIPLE_COMMANDS: &str = "cannot insert multiple commands into a prepared statement";

/// Part 04 §8.1 SQLSTATE table, applied to an error of `prepare`, `DECLARE`, `FETCH` or `simple_query`.
/// `has_columns` is `false` only when `prepare` succeeded and the statement has no result columns (a write);
/// pass `true` for errors raised by `prepare` itself. `cancelled` = `cancel` was called for this execution.
pub fn map_sql_error(e: &tokio_postgres::Error, has_columns: bool, cancelled: bool) -> AppError {
    match e.as_db_error() {
        None => crate::db::no_sqlstate_err(e),
        Some(db) => {
            let position = match db.position() {
                Some(ErrorPosition::Original(n)) => Some(*n),
                _ => None,
            };
            map_sqlstate(db.code().code(), db.message(), position, has_columns, cancelled)
        }
    }
}

/// The table itself, on plain values (unit-tested without a server).
pub fn map_sqlstate(sqlstate: &str, message: &str, position: Option<u32>, has_columns: bool, cancelled: bool) -> AppError {
    match sqlstate {
        "42601" if message.contains(MULTIPLE_COMMANDS) => AppError::invalid("sql", "multipleStatements"),
        "25006" | "55000" => AppError::ReadOnlyViolation,
        "42501" if !has_columns => AppError::ReadOnlyViolation,
        "57014" if cancelled => AppError::Cancelled,
        "57014" => AppError::QueryTimeout { timeout_ms: TIMEOUT_MS },
        _ => AppError::SqlError { sqlstate: sqlstate.to_owned(), message: message.to_owned(), position },
    }
}

/// Values longer than 2,000 chars → first 2,000 chars + "…".
pub fn cut_cell(v: &str) -> String {
    match v.char_indices().nth(CELL_MAX_CHARS) {
        Some((byte, _)) => format!("{}…", &v[..byte]),
        None => v.to_owned(),
    }
}

/// First keyword of the statement, ASCII-uppercased (known limit, Part 04 §8.1 step 3).
/// Leading whitespace, `--` line comments and `/* */` block comments are skipped.
pub fn command_tag(sql: &str) -> String {
    let mut s = sql;
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map_or("", |(_, r)| r);
        } else {
            break;
        }
    }
    s.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_cell_adds_ellipsis() {
        assert_eq!(cut_cell("abc"), "abc");
        let exact = "報".repeat(2_000);
        assert_eq!(cut_cell(&exact), exact);
        let cut = cut_cell(&"報".repeat(2_001));
        assert_eq!(cut.chars().count(), 2_001);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().filter(|c| *c == '報').count(), 2_000);
    }

    #[test]
    fn command_tag_first_keyword() {
        assert_eq!(command_tag("set work_mem = '8MB'"), "SET");
        assert_eq!(command_tag("  \n\tDO $$BEGIN END$$"), "DO");
        assert_eq!(command_tag("-- note\n/* block */ Analyze v_mail"), "ANALYZE");
        assert_eq!(command_tag("(SELECT 1)"), "");
        assert_eq!(command_tag(""), "");
    }

    #[test]
    fn error_mapping_table() {
        let multi = "cannot insert multiple commands into a prepared statement";
        assert_eq!(map_sqlstate("42601", multi, None, true, false), AppError::invalid("sql", "multipleStatements"));
        assert_eq!(map_sqlstate("25006", "read-only", None, false, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("25006", "read-only", None, true, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("55000", "cannot delete from view", None, true, false), AppError::ReadOnlyViolation);
        assert_eq!(map_sqlstate("42501", "permission denied", None, false, false), AppError::ReadOnlyViolation);
        assert_eq!(
            map_sqlstate("42501", "permission denied for table messages", None, true, false),
            AppError::SqlError { sqlstate: "42501".into(), message: "permission denied for table messages".into(), position: None }
        );
        assert_eq!(map_sqlstate("57014", "canceling statement", None, true, true), AppError::Cancelled);
        assert_eq!(map_sqlstate("57014", "canceling statement", None, true, false), AppError::QueryTimeout { timeout_ms: 30_000 });
        assert_eq!(
            map_sqlstate("42601", "syntax error at or near \"SELEC\"", Some(1), true, false),
            AppError::SqlError { sqlstate: "42601".into(), message: "syntax error at or near \"SELEC\"".into(), position: Some(1) }
        );
        assert!(matches!(map_sqlstate("428C9", "generated", None, false, false), AppError::SqlError { .. }));
    }
}
