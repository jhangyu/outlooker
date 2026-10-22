//! Part 04 §5.6 HTML sanitize + normalize (ammonia allow-list) and the cid / data: image rewrite.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use ammonia::{Builder, UrlRelative};
use base64::Engine as _;

const TAGS: &[&str] = &[
    "a", "abbr", "b", "blockquote", "br", "caption", "center", "cite", "code", "col", "colgroup", "dd", "del", "div",
    "dl", "dt", "em", "font", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "i", "img", "ins", "li", "ol", "p", "pre", "q",
    "s", "small", "span", "strike", "strong", "sub", "sup", "table", "tbody", "td", "tfoot", "th", "thead", "tr", "u",
    "ul",
];
const CLEAN_CONTENT_TAGS: &[&str] = &[
    "script", "style", "head", "title", "iframe", "frame", "frameset", "object", "embed", "applet", "form", "input",
    "button", "select", "option", "textarea", "svg", "math", "noscript", "template", "link", "meta", "base", "audio",
    "video", "canvas",
];
const GENERIC_ATTRIBUTES: &[&str] = &["style", "title", "dir", "lang", "align", "valign", "width", "height", "bgcolor"];
const TAG_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href"]),
    ("img", &["src", "alt", "border", "hspace", "vspace"]),
    ("font", &["color", "face", "size"]),
    ("table", &["border", "cellpadding", "cellspacing"]),
    ("td", &["colspan", "rowspan", "nowrap"]),
    ("th", &["colspan", "rowspan", "nowrap"]),
    ("col", &["span"]),
    ("colgroup", &["span"]),
    ("ol", &["start", "type"]),
    ("li", &["value", "type"]),
];
/// `cid` and `data` must pass ammonia's scheme check (it runs before `attribute_filter`); the filter then
/// rewrites them, so neither survives in the output.
const URL_SCHEMES: &[&str] = &["http", "https", "mailto", "cid", "data", "outlooker-inline"];
const STYLE_PROPERTIES: &[&str] = &[
    "color", "background-color", "font", "font-family", "font-size", "font-weight", "font-style", "font-variant",
    "text-decoration", "text-align", "text-indent", "text-transform", "vertical-align", "line-height",
    "letter-spacing", "word-spacing", "white-space", "margin", "margin-top", "margin-right", "margin-bottom",
    "margin-left", "padding", "padding-top", "padding-right", "padding-bottom", "padding-left", "border",
    "border-top", "border-right", "border-bottom", "border-left", "border-width", "border-style", "border-color",
    "border-collapse", "border-spacing", "width", "height", "max-width", "min-width", "max-height", "min-height",
    "display", "list-style-type", "direction",
];
const STYLE_FORBIDDEN: &[&str] = &["url(", "expression(", "javascript:", "@import", "behavior", "\\", "/*", "<"];
const DATA_IMAGE_PREFIXES: &[&str] = &[
    "data:image/png;base64,", "data:image/jpeg;base64,", "data:image/gif;base64,", "data:image/bmp;base64,",
    "data:image/webp;base64,",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineSource {
    /// Index into `RawMail.attachments`.
    Cid { cid: String, attachment: usize },
    Data(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRef { pub ordinal: i32, pub source: InlineSource }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized { pub html: String, pub refs: Vec<InlineRef> }

/// cid normalization for matching: strip surrounding `<` `>`, trim, ASCII-lowercase.
pub fn normalize_cid(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('<').unwrap_or(t);
    let t = t.strip_suffix('>').unwrap_or(t);
    t.trim().to_ascii_lowercase()
}

#[derive(Default)]
struct RefState { refs: Vec<InlineRef>, by_cid: HashMap<String, i32> }

/// Sanitizes one message body. `cid_map`: normalized content id → attachment index.
pub fn sanitize(html: &str, cid_map: &HashMap<String, usize>) -> Sanitized {
    let state = Arc::new(Mutex::new(RefState::default()));
    let cids = Arc::new(cid_map.clone());
    let filter_state = Arc::clone(&state);
    let mut b = Builder::empty();
    b.tags(TAGS.iter().copied().collect())
        .clean_content_tags(CLEAN_CONTENT_TAGS.iter().copied().collect())
        .generic_attributes(GENERIC_ATTRIBUTES.iter().copied().collect())
        .tag_attributes(TAG_ATTRIBUTES.iter().map(|(t, a)| (*t, a.iter().copied().collect::<HashSet<_>>())).collect())
        .url_schemes(URL_SCHEMES.iter().copied().collect())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .filter_style_properties(STYLE_PROPERTIES.iter().copied().collect())
        .attribute_filter(move |element, attribute, value| filter(element, attribute, value, &cids, &filter_state));
    let html = b.clean(html).to_string().trim().to_string();
    let refs = std::mem::take(&mut state.lock().unwrap().refs);
    Sanitized { html, refs }
}

fn filter<'u>(
    element: &str,
    attribute: &str,
    value: &'u str,
    cids: &HashMap<String, usize>,
    state: &Mutex<RefState>,
) -> Option<Cow<'u, str>> {
    match (element, attribute) {
        ("a", "href") => {
            let l = value.trim().to_ascii_lowercase();
            if l.starts_with("http:") || l.starts_with("https:") || l.starts_with("mailto:") {
                Some(Cow::Borrowed(value))
            } else {
                None
            }
        }
        ("img", "src") => {
            let v = value.trim();
            let lower = v.to_ascii_lowercase();
            let mut st = state.lock().unwrap();
            if let Some(rest) = lower.strip_prefix("cid:") {
                let cid = normalize_cid(rest);
                if let Some(&ordinal) = st.by_cid.get(&cid) {
                    return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
                }
                let &attachment = cids.get(&cid)?;
                let ordinal = st.refs.len() as i32;
                st.by_cid.insert(cid.clone(), ordinal);
                st.refs.push(InlineRef { ordinal, source: InlineSource::Cid { cid, attachment } });
                return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
            }
            if let Some(prefix) = DATA_IMAGE_PREFIXES.iter().find(|p| lower.starts_with(**p)) {
                let payload: String = v[prefix.len()..].chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = base64::engine::general_purpose::STANDARD.decode(payload.as_bytes()).ok()?;
                let ordinal = st.refs.len() as i32;
                st.refs.push(InlineRef { ordinal, source: InlineSource::Data(bytes) });
                return Some(Cow::Owned(format!("outlooker-inline:{ordinal}")));
            }
            None // remote http(s) and everything else: never fetched
        }
        (_, "style") => {
            let l = value.to_ascii_lowercase();
            if STYLE_FORBIDDEN.iter().any(|f| l.contains(f)) { None } else { Some(Cow::Borrowed(value)) }
        }
        _ => Some(Cow::Borrowed(value)),
    }
}

/// Removes ` src="outlooker-inline:<ordinal>"` (an image that was skipped or failed); the `<img>` stays.
pub fn drop_src(html: &str, ordinal: i32) -> String {
    html.replace(&format!(" src=\"outlooker-inline:{ordinal}\""), "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    /// The forbidden patterns of the sanitizer acceptance, matched without a regex crate.
    pub(crate) fn hostile_violations(out: &str) -> Vec<String> {
        let lower = out.to_ascii_lowercase();
        let mut found = Vec::new();
        for p in ["<script", "javascript:", "url(", "<iframe", "<form", "position", "src=\"http"] {
            if lower.contains(p) {
                found.push(p.to_string());
            }
        }
        // on[a-z]+=
        let bytes = lower.as_bytes();
        for i in 0..bytes.len().saturating_sub(2) {
            if bytes[i] == b'o' && bytes[i + 1] == b'n' && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
                let mut j = i + 2;
                while j < bytes.len() && bytes[j].is_ascii_lowercase() { j += 1; }
                if j > i + 2 && j < bytes.len() && bytes[j] == b'=' {
                    found.push(lower[i..=j].to_string());
                }
            }
        }
        found
    }

    #[test]
    fn hostile_corpus_is_clean() {
        let out = sanitize(&golden("hostile.html"), &HashMap::new());
        assert_eq!(hostile_violations(&out.html), Vec::<String>::new(), "output: {}", out.html);
        assert!(out.html.contains("Quarterly numbers"), "text content kept: {}", out.html);
        assert!(!out.html.contains("<base") && !out.html.contains("<meta"));
    }

    #[test]
    fn golden_plain() {
        let out = sanitize(&golden("plain.html"), &HashMap::new());
        assert_eq!(out.html, golden("plain.expected.html").trim_end_matches('\n'));
        assert!(out.refs.is_empty());
    }

    #[test]
    fn cid_and_data_ordinals() {
        let mut cids = HashMap::new();
        cids.insert("logo@x".to_string(), 0usize);
        let html = r#"<p><img src="cid:LOGO@X"><img src="data:image/png;base64,iVBORw0K"><img src="cid:logo@x"><img src="cid:missing"><img src="https://t.example/p.gif"><a href="javascript:alert(1)">x</a><a href="cid:logo@x">y</a><a href="https://ok.example/">z</a></p>"#;
        let out = sanitize(html, &cids);
        assert_eq!(
            out.html,
            r#"<p><img src="outlooker-inline:0"><img src="outlooker-inline:1"><img src="outlooker-inline:0"><img><img><a rel="noopener noreferrer">x</a><a rel="noopener noreferrer">y</a><a href="https://ok.example/" rel="noopener noreferrer">z</a></p>"#
        );
        assert_eq!(out.refs.len(), 2);
        assert_eq!(drop_src(&out.html, 0).matches("outlooker-inline:0").count(), 0);
    }
}
