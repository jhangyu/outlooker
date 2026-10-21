//! SearchQuery → one SQL predicate over `messages m` (spec Part 04 §7.3). The predicate text is built only from the
//! constant fragments below and `$n` placeholders; every user value travels in `params`.
use tokio_postgres::types::ToSql;

use crate::dto::error::AppError;
use crate::dto::search::{ConditionGroup, ConditionNode, GroupOp, RecipientKind, SearchQuery};

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Text(String),
    Bool(bool),
    TextArray(Vec<String>),
    BigintArray(Vec<i64>),
}

impl Param {
    pub fn as_sql(&self) -> &(dyn ToSql + Sync) {
        match self {
            Param::Text(v) => v,
            Param::Bool(v) => v,
            Param::TextArray(v) => v,
            Param::BigintArray(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub predicate: String,
    pub params: Vec<Param>,
    pub indexable: bool,
    pub short_query: bool,
}

pub const MAX_TERMS: usize = 10;
pub const MAX_DEPTH: usize = 5;
pub const MAX_LEAVES: usize = 50;
pub const MAX_VALUE_CHARS: usize = 500;
pub const MIN_INDEXABLE_CHARS: usize = 3;
pub const MAX_FOLDER_PATH_CHARS: usize = 4096;
pub const MAX_PST_IDS: usize = 100;

/// `\` → `\\`, `%` → `\%`, `_` → `\_` (backslash is the default LIKE escape character).
pub fn escape_like(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn like(v: &str) -> String {
    format!("%{}%", escape_like(v))
}

struct Out {
    sql: String,
    indexable: bool,
}

struct Builder {
    params: Vec<Param>,
    short: bool,
}

impl Builder {
    fn param(&mut self, p: Param) -> String {
        self.params.push(p);
        format!("${}", self.params.len())
    }

    fn text_value(&mut self, value: &str, path: &str) -> Result<(String, bool), AppError> {
        let v = value.trim();
        let n = v.chars().count();
        if n == 0 {
            return Err(AppError::invalid(&format!("{path}.value"), "empty"));
        }
        if n > MAX_VALUE_CHARS {
            return Err(AppError::invalid(&format!("{path}.value"), "tooLong"));
        }
        let indexable = n >= MIN_INDEXABLE_CHARS;
        self.short |= !indexable;
        Ok((v.to_owned(), indexable))
    }

    fn node(&mut self, node: &ConditionNode, path: &str, depth: usize) -> Result<Out, AppError> {
        match node {
            ConditionNode::Group { op, children } => self.group(*op, children, path, depth),
            ConditionNode::Sender { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                Ok(Out { sql: format!("m.sender_search ILIKE {p}"), indexable: ix })
            }
            ConditionNode::Subject { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                Ok(Out { sql: format!("m.subject ILIKE {p}"), indexable: ix })
            }
            ConditionNode::Body { value } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                if v.chars().any(|c| c.is_ascii_alphabetic()) {
                    let q = self.param(Param::Text(v));
                    // Stored column first: rows matching by stem skip the ILIKE (spec Part 04 §7.3, §7.4).
                    Ok(Out { sql: format!("(m.body_tsv @@ websearch_to_tsquery('english', {q}) OR m.body_text ILIKE {p})"), indexable: ix })
                } else {
                    Ok(Out { sql: format!("m.body_text ILIKE {p}"), indexable: ix })
                }
            }
            ConditionNode::Recipient { value, kinds } => {
                let (v, ix) = self.text_value(value, path)?;
                let p = self.param(Param::Text(like(&v)));
                let mut names: Vec<String> = kinds
                    .iter()
                    .map(|k| match k {
                        RecipientKind::To => "to",
                        RecipientKind::Cc => "cc",
                        RecipientKind::Bcc => "bcc",
                    })
                    .map(str::to_owned)
                    .collect();
                names.sort();
                names.dedup();
                if names.is_empty() || names.len() == 3 {
                    Ok(Out { sql: format!("m.recipients_search ILIKE {p}"), indexable: ix })
                } else {
                    let k = self.param(Param::TextArray(names));
                    Ok(Out {
                        sql: format!(
                            "(m.recipients_search ILIKE {p} AND EXISTS (SELECT 1 FROM message_recipients r WHERE r.message_id = m.id \
                             AND r.kind = ANY({k}::text[]) AND r.search_text ILIKE {p}))"
                        ),
                        indexable: ix,
                    })
                }
            }
            ConditionNode::DateRange { from, to } => {
                if from.is_none() && to.is_none() {
                    return Err(AppError::invalid(path, "empty"));
                }
                let mut parts = Vec::new();
                match from {
                    Some(f) => {
                        if !is_rfc3339(f) {
                            return Err(AppError::invalid(&format!("{path}.from"), "invalidInstant"));
                        }
                        let p = self.param(Param::Text(f.clone()));
                        parts.push(format!("m.sort_at >= {p}::text::timestamptz"));
                    }
                    None => parts.push("m.sort_at > '-infinity'".to_owned()),
                }
                if let Some(t) = to {
                    if !is_rfc3339(t) {
                        return Err(AppError::invalid(&format!("{path}.to"), "invalidInstant"));
                    }
                    let p = self.param(Param::Text(t.clone()));
                    parts.push(format!("m.sort_at < {p}::text::timestamptz"));
                }
                Ok(Out { sql: format!("({})", parts.join(" AND ")), indexable: false })
            }
            ConditionNode::Folder { folder_path, include_subfolders } => {
                let n = folder_path.chars().count();
                if n == 0 {
                    return Err(AppError::invalid(&format!("{path}.folderPath"), "empty"));
                }
                if n > MAX_FOLDER_PATH_CHARS {
                    return Err(AppError::invalid(&format!("{path}.folderPath"), "tooLong"));
                }
                let p = self.param(Param::Text(folder_path.clone()));
                let sub = if *include_subfolders {
                    let pp = self.param(Param::Text(format!("{}/%", escape_like(folder_path))));
                    format!(" OR f.path LIKE {pp}")
                } else {
                    String::new()
                };
                Ok(Out {
                    sql: format!(
                        "EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id AND o.folder_id = ANY \
                         (SELECT f.id FROM folders f WHERE f.path = {p}{sub}))"
                    ),
                    indexable: false,
                })
            }
            ConditionNode::HasAttachment { value } => {
                let p = self.param(Param::Bool(*value));
                Ok(Out { sql: format!("m.has_attachments = {p}"), indexable: false })
            }
            ConditionNode::SourcePst { pst_ids } => {
                let mut unique = pst_ids.clone();
                unique.sort_unstable();
                unique.dedup();
                if pst_ids.is_empty() || pst_ids.len() > MAX_PST_IDS || unique.len() != pst_ids.len() || pst_ids.iter().any(|&i| i < 1) {
                    return Err(AppError::invalid(&format!("{path}.pstIds"), "invalid"));
                }
                let p = self.param(Param::BigintArray(pst_ids.clone()));
                Ok(Out {
                    sql: format!("EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id AND o.pst_id = ANY({p}::bigint[]))"),
                    indexable: false,
                })
            }
        }
    }

    fn group(&mut self, op: GroupOp, children: &[ConditionNode], path: &str, depth: usize) -> Result<Out, AppError> {
        if depth > MAX_DEPTH {
            return Err(AppError::invalid(path, "tooDeep"));
        }
        if children.is_empty() {
            return Err(AppError::invalid(&format!("{path}.children"), "empty"));
        }
        let mut parts = Vec::with_capacity(children.len());
        let mut any = false;
        let mut all = true;
        for (i, c) in children.iter().enumerate() {
            let out = self.node(c, &format!("{path}.children[{i}]"), depth + 1)?;
            any |= out.indexable;
            all &= out.indexable;
            parts.push(out.sql);
        }
        let (joiner, indexable) = match op {
            GroupOp::And => (" AND ", any),
            GroupOp::Or => (" OR ", all),
        };
        Ok(Out { sql: format!("({})", parts.join(joiner)), indexable })
    }
}

fn leaves(nodes: &[ConditionNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            ConditionNode::Group { children, .. } => leaves(children),
            _ => 1,
        })
        .sum()
}

pub fn compile(query: &SearchQuery) -> Result<Compiled, AppError> {
    let mut b = Builder { params: Vec::new(), short: false };
    let out = match query {
        SearchQuery::Simple { text } => {
            let terms: Vec<&str> = text.split_whitespace().collect();
            if terms.is_empty() {
                return Err(AppError::invalid("query.text", "empty"));
            }
            if terms.len() > MAX_TERMS {
                return Err(AppError::invalid("query.text", "tooManyTerms"));
            }
            if terms.iter().any(|t| t.chars().count() > MAX_VALUE_CHARS) {
                return Err(AppError::invalid("query.text", "tooLong"));
            }
            let children = terms
                .iter()
                .map(|t| ConditionNode::Group {
                    op: GroupOp::Or,
                    children: vec![
                        ConditionNode::Sender { value: (*t).to_owned() },
                        ConditionNode::Recipient { value: (*t).to_owned(), kinds: Vec::new() },
                        ConditionNode::Subject { value: (*t).to_owned() },
                        ConditionNode::Body { value: (*t).to_owned() },
                    ],
                })
                .collect::<Vec<_>>();
            b.group(GroupOp::And, &children, "query.text", 1)?
        }
        SearchQuery::Dsl { root } => {
            let ConditionGroup { op, children } = root;
            if leaves(children) > MAX_LEAVES {
                return Err(AppError::invalid("query.root", "tooManyLeaves"));
            }
            b.group(*op, children, "query.root", 1)?
        }
    };
    Ok(Compiled { predicate: out.sql, params: b.params, indexable: out.indexable, short_query: b.short })
}

/// RFC 3339 date-time (`YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`); `T`/`Z` also lowercase (RFC 3339 §5.6 note).
pub fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r.clone()).is_some_and(|x| x.iter().all(u8::is_ascii_digit));
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(99);
    if b.len() < 20 || !digits(0..4) || b[4] != b'-' || !digits(5..7) || b[7] != b'-' || !digits(8..10) {
        return false;
    }
    if !matches!(b[10], b'T' | b't') || !digits(11..13) || b[13] != b':' || !digits(14..16) || b[16] != b':' || !digits(17..19) {
        return false;
    }
    if !(1..=12).contains(&num(5..7)) || !(1..=31).contains(&num(8..10)) || num(11..13) > 23 || num(14..16) > 59 || num(17..19) > 60 {
        return false;
    }
    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match &b[i..] {
        [b'Z' | b'z'] => true,
        [sign, ..] if (*sign == b'+' || *sign == b'-') && b.len() == i + 6 => {
            digits(i + 1..i + 3) && b[i + 3] == b':' && digits(i + 4..i + 6) && num(i + 1..i + 3) <= 23 && num(i + 4..i + 6) <= 59
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::search::ConditionNode as N;

    fn dsl(op: GroupOp, children: Vec<ConditionNode>) -> SearchQuery {
        SearchQuery::Dsl { root: ConditionGroup { op, children } }
    }

    fn text_params(c: &Compiled) -> Vec<String> {
        c.params
            .iter()
            .filter_map(|p| match p {
                Param::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_user_value_in_sql() {
        let marker = "zq'x%_\\";
        let cases: Vec<(ConditionNode, Vec<Param>)> = vec![
            (N::Sender { value: marker.into() }, vec![Param::Text(like(marker))]),
            (N::Recipient { value: marker.into(), kinds: vec![RecipientKind::Cc] }, vec![Param::Text(like(marker)), Param::TextArray(vec!["cc".into()])]),
            (N::Subject { value: marker.into() }, vec![Param::Text(like(marker))]),
            (N::Body { value: marker.into() }, vec![Param::Text(like(marker)), Param::Text(marker.into())]),
            (
                N::DateRange { from: Some("2020-01-01T00:00:00Z".into()), to: Some("2021-01-01T00:00:00+08:00".into()) },
                vec![Param::Text("2020-01-01T00:00:00Z".into()), Param::Text("2021-01-01T00:00:00+08:00".into())],
            ),
            (
                N::Folder { folder_path: format!("Inbox/{marker}"), include_subfolders: true },
                vec![Param::Text(format!("Inbox/{marker}")), Param::Text(format!("Inbox/{}/%", escape_like(marker)))],
            ),
            (N::HasAttachment { value: true }, vec![Param::Bool(true)]),
            (N::SourcePst { pst_ids: vec![424242, 7] }, vec![Param::BigintArray(vec![424242, 7])]),
        ];
        for (node, expected) in cases {
            let c = compile(&dsl(GroupOp::And, vec![node.clone()])).unwrap();
            for needle in ["zq", "2020-01-01", "2021", "Inbox", "424242"] {
                assert!(!c.predicate.contains(needle), "{needle} leaked into {}", c.predicate);
            }
            // the only quoted literals allowed are the constants of the fragment table
            let stripped = c.predicate.replace("'english'", "").replace("' '", "").replace("'-infinity'", "");
            assert!(!stripped.contains('\''), "{}", c.predicate);
            assert_eq!(c.params, expected, "{node:?}");
        }
    }

    #[test]
    fn like_escapes_literally() {
        assert_eq!(escape_like(r"50%_off\now"), r"50\%\_off\\now");
        assert_eq!(like("a%b"), r"%a\%b%");
        let c = compile(&SearchQuery::Simple { text: "100%".into() }).unwrap();
        assert_eq!(text_params(&c)[0], r"%100\%%");
    }

    #[test]
    fn limits_paths() {
        let err = |q: SearchQuery| compile(&q).unwrap_err();
        let inv = |f: &str, r: &str| AppError::invalid(f, r);
        assert_eq!(err(SearchQuery::Simple { text: "  ".into() }), inv("query.text", "empty"));
        assert_eq!(err(SearchQuery::Simple { text: "a ".repeat(11) }), inv("query.text", "tooManyTerms"));
        assert_eq!(err(SearchQuery::Simple { text: "a".repeat(501) }), inv("query.text", "tooLong"));
        assert_eq!(err(dsl(GroupOp::And, vec![])), inv("query.root.children", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Subject { value: " ".into() }])), inv("query.root.children[0].value", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Body { value: "x".repeat(501) }])), inv("query.root.children[0].value", "tooLong"));
        let mut deep = N::Subject { value: "abc".into() };
        for _ in 0..5 {
            deep = N::Group { op: GroupOp::And, children: vec![deep] };
        }
        assert_eq!(err(dsl(GroupOp::And, vec![deep])), inv("query.root.children[0].children[0].children[0].children[0].children[0]", "tooDeep"));
        let many: Vec<_> = (0..51).map(|_| N::HasAttachment { value: true }).collect();
        assert_eq!(err(dsl(GroupOp::And, many)), inv("query.root", "tooManyLeaves"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::DateRange { from: None, to: None }])), inv("query.root.children[0]", "empty"));
        assert_eq!(
            err(dsl(GroupOp::And, vec![N::HasAttachment { value: true }, N::DateRange { from: Some("2020-13-01T00:00:00Z".into()), to: None }])),
            inv("query.root.children[1].from", "invalidInstant")
        );
        assert_eq!(err(dsl(GroupOp::And, vec![N::DateRange { from: None, to: Some("yesterday".into()) }])), inv("query.root.children[0].to", "invalidInstant"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Folder { folder_path: String::new(), include_subfolders: false }])), inv("query.root.children[0].folderPath", "empty"));
        assert_eq!(err(dsl(GroupOp::And, vec![N::Folder { folder_path: "f".repeat(4097), include_subfolders: false }])), inv("query.root.children[0].folderPath", "tooLong"));
        for ids in [vec![], vec![0], vec![3, 3], (1..=101).collect()] {
            assert_eq!(err(dsl(GroupOp::And, vec![N::SourcePst { pst_ids: ids }])), inv("query.root.children[0].pstIds", "invalid"));
        }
        // depth 5 is allowed
        let mut ok = N::Subject { value: "abc".into() };
        for _ in 0..3 {
            ok = N::Group { op: GroupOp::And, children: vec![ok] };
        }
        compile(&dsl(GroupOp::And, vec![ok])).unwrap();
    }

    #[test]
    fn worked_example_predicate() {
        let q = dsl(
            GroupOp::And,
            vec![
                N::Group { op: GroupOp::Or, children: vec![N::Sender { value: "contoso.com".into() }, N::Recipient { value: "alice".into(), kinds: vec![] }] },
                N::DateRange { from: Some("2022-12-31T16:00:00Z".into()), to: Some("2023-12-31T16:00:00Z".into()) },
                N::Body { value: "報價".into() },
                N::HasAttachment { value: true },
                N::SourcePst { pst_ids: vec![3, 7] },
            ],
        );
        let c = compile(&q).unwrap();
        assert_eq!(
            c.predicate,
            "((m.sender_search ILIKE $1 OR m.recipients_search ILIKE $2) AND (m.sort_at >= $3::text::timestamptz AND m.sort_at < $4::text::timestamptz) \
             AND m.body_text ILIKE $5 AND m.has_attachments = $6 AND EXISTS (SELECT 1 FROM message_occurrences o WHERE o.message_id = m.id \
             AND o.pst_id = ANY($7::bigint[])))"
        );
        assert_eq!(
            c.params,
            vec![
                Param::Text("%contoso.com%".into()),
                Param::Text("%alice%".into()),
                Param::Text("2022-12-31T16:00:00Z".into()),
                Param::Text("2023-12-31T16:00:00Z".into()),
                Param::Text("%報價%".into()),
                Param::Bool(true),
                Param::BigintArray(vec![3, 7]),
            ]
        );
        assert!(c.short_query, "報價 has 2 characters");
        assert!(c.indexable, "an AND with one indexable child is indexable");
    }

    #[test]
    fn simple_query_shape_and_tsquery_rule() {
        let c = compile(&SearchQuery::Simple { text: "invoice 報價單".into() }).unwrap();
        assert_eq!(
            c.predicate,
            "((m.sender_search ILIKE $1 OR m.recipients_search ILIKE $2 OR m.subject ILIKE $3 OR (m.body_tsv @@ \
             websearch_to_tsquery('english', $5) OR m.body_text ILIKE $4)) AND (m.sender_search ILIKE $6 OR m.recipients_search ILIKE $7 OR \
             m.subject ILIKE $8 OR m.body_text ILIKE $9))"
        );
        assert!(c.indexable && !c.short_query);
        let short = compile(&SearchQuery::Simple { text: "ab".into() }).unwrap();
        assert!(!short.indexable && short.short_query);
    }

    #[test]
    fn rfc3339_validation() {
        for ok in ["2023-01-01T00:00:00Z", "2023-01-01t10:20:30.123456z", "2023-06-30T23:59:60+08:00", "1999-12-31T00:00:00-05:30"] {
            assert!(is_rfc3339(ok), "{ok}");
        }
        for bad in ["2023-01-01", "2023-01-01 00:00:00Z", "2023-00-01T00:00:00Z", "2023-01-01T24:00:00Z", "2023-01-01T00:00:00", "2023-01-01T00:00:00.Z", "2023-01-01T00:00:00+0800"] {
            assert!(!is_rfc3339(bad), "{bad}");
        }
    }
}
