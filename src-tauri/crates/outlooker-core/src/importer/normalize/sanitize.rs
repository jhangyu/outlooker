//! Part 04 §5.6 HTML sanitize + normalize (ammonia allow-list) and the cid / data: image rewrite.
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use ammonia::{Builder, UrlRelative};
use base64::Engine as _;

/// Content skeleton. `div` and `span` survive ammonia only so `flatten` can turn them into line breaks / `<code>`.
const TAGS: &[&str] = &[
    "p", "br", "hr", "blockquote", "pre", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "dl", "dt", "dd", "b",
    "strong", "i", "em", "u", "s", "del", "strike", "sub", "sup", "code", "a", "img", "table", "thead", "tbody",
    "tfoot", "tr", "td", "th", "caption", "font", "div", "span",
];
const CLEAN_CONTENT_TAGS: &[&str] = &[
    "script", "style", "head", "title", "iframe", "frame", "frameset", "object", "embed", "applet", "form", "input",
    "button", "select", "option", "textarea", "svg", "math", "noscript", "template", "link", "meta", "base", "audio",
    "video", "canvas",
];
const GENERIC_ATTRIBUTES: &[&str] = &["style"];
/// `font face` is a transient input: the filter turns it into `face="monospace"` or drops it, and `flatten`
/// turns the former into `<code>`.
const TAG_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href"]),
    ("img", &["src", "alt"]),
    ("font", &["color", "size", "face"]),
    ("td", &["colspan", "rowspan"]),
    ("th", &["colspan", "rowspan"]),
];
/// `cid` and `data` must pass ammonia's scheme check (it runs before `attribute_filter`); the filter then
/// rewrites them, so neither survives in the output.
const URL_SCHEMES: &[&str] = &["http", "https", "mailto", "cid", "data", "outlooker-inline"];
/// `font-family` is a transient input: the filter reduces it to `font-family:monospace` or drops it, and
/// `flatten` turns the former into `<code>`.
const STYLE_PROPERTIES: &[&str] =
    &["color", "background-color", "font-weight", "font-style", "text-decoration", "font-family"];
const MONO_FONTS: &[&str] = &[
    "consolas", "courier", "cascadia mono", "cascadia code", "lucida console", "menlo", "monaco", "sf mono",
    "monospace",
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
    let html = flatten(&b.clean(html).to_string());
    let refs = std::mem::take(&mut state.lock().unwrap_or_else(|p| p.into_inner()).refs);
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
            let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
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
        ("font", "face") => is_mono(value).then_some(Cow::Borrowed("monospace")),
        (_, "style") => {
            let l = value.to_ascii_lowercase();
            if STYLE_FORBIDDEN.iter().any(|f| l.contains(f)) {
                return None;
            }
            // Mono if any font declaration (font-family, font shorthand, mso-*-font-family, mso-*-theme-font) names one.
            let mut mono = false;
            let mut kept: Vec<&str> = Vec::new();
            for decl in value.split(';') {
                let (prop, val) = decl.split_once(':').unwrap_or((decl, ""));
                let prop = prop.trim().to_ascii_lowercase();
                if prop == "font-family" || prop == "font" || (prop.starts_with("mso-") && prop.contains("font")) {
                    mono |= is_mono(val);
                }
                if !decl.trim().is_empty() && prop != "font-family" {
                    kept.push(decl);
                }
            }
            if mono {
                kept.push(MONO_STYLE);
            }
            Some(Cow::Owned(kept.join(";")))
        }
        _ => Some(Cow::Borrowed(value)),
    }
}

fn is_mono(font: &str) -> bool {
    let l = font.to_ascii_lowercase();
    MONO_FONTS.iter().any(|f| l.contains(f))
}

const MONO_STYLE: &str = "font-family:monospace";
const MONO_FACE: &str = " face=\"monospace\"";
const VOID: &[&str] = &["br", "hr", "img"];
const BLOCKS: &[&str] = &[
    "p", "hr", "blockquote", "pre", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "dl", "dt", "dd", "table",
    "thead", "tbody", "tfoot", "tr", "td", "th", "caption",
];
/// Elements where a `<code>` child would be invalid HTML or pointless: the marker is dropped, not wrapped.
const NO_CODE: &[&str] = &["ul", "ol", "dl", "table", "thead", "tbody", "tfoot", "tr", "pre", "code", "hr", "br", "img"];
/// End tags before which a trailing `<br>` renders nothing.
const TRIM_BEFORE: &[&str] = &["td", "th", "li", "blockquote", "p"];

#[derive(PartialEq, Clone, Copy)]
enum Kind { Inline, Block, Div }

struct Frame {
    name: String,
    kind: Kind,
    wrapped: bool,
    /// Inline content (or a `<br>`) since the last line break at this level.
    pending: bool,
}

fn trim_trailing_br(out: &mut String) {
    loop {
        let t = out.trim_end();
        if t.ends_with("<br>") { out.truncate(t.len() - 4) } else { break }
    }
}

/// Nearest enclosing block or div (inline wrappers do not own line breaks).
fn nearest(stack: &mut [Frame]) -> &mut Frame {
    let at = stack.iter().rposition(|f| f.kind != Kind::Inline).unwrap_or(0);
    &mut stack[at]
}

/// Post-pass over ammonia's output (balanced, attributes double-quoted, `<` in text escaped): `div` becomes line
/// breaks, mono markers become `<code>`, a trailing `<br>` before a closing block is dropped.
fn flatten(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut stack = vec![Frame { name: String::new(), kind: Kind::Block, wrapped: false, pending: false }];
    let mut last_br = false;
    let mut i = 0;
    while i < html.len() {
        let rest = &html[i..];
        if !rest.starts_with('<') {
            let end = rest.find('<').unwrap_or(rest.len());
            let text = &rest[..end];
            out.push_str(text);
            if !text.trim().is_empty() {
                nearest(&mut stack).pending = true;
                last_br = false;
            }
            i += end;
            continue;
        }
        let mut quoted = false;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                if c == '"' {
                    quoted = !quoted;
                }
                c == '>' && !quoted
            })
            .map_or(rest.len(), |(n, _)| n + 1);
        let tag = &rest[..end];
        i += end;
        let closing = tag.starts_with("</");
        let name: String = tag
            .trim_start_matches('<')
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let name = name.as_str();
        if closing {
            let Some(at) = stack.iter().rposition(|f| f.name == name).filter(|&at| at > 0) else {
                out.push_str(tag);
                continue;
            };
            let frame = stack.swap_remove(at);
            stack.truncate(at);
            if TRIM_BEFORE.contains(&name) {
                trim_trailing_br(&mut out);
            }
            if frame.kind == Kind::Div {
                let had_br = last_br && out.ends_with("<br>");
                if had_br {
                    out.truncate(out.len() - 4);
                }
                if frame.pending || had_br {
                    out.push_str("<br>");
                    last_br = true;
                }
                nearest(&mut stack).pending = false;
                continue;
            }
            if frame.wrapped {
                out.push_str("</code>");
            }
            if name != "span" {
                out.push_str(tag);
            }
            last_br = false;
            if frame.kind == Kind::Block {
                nearest(&mut stack).pending = false;
            }
            continue;
        }
        if name == "div" {
            let parent = nearest(&mut stack);
            if parent.pending {
                out.push_str("<br>");
                last_br = true;
                parent.pending = false;
            }
            stack.push(Frame { name: name.into(), kind: Kind::Div, wrapped: false, pending: false });
            continue;
        }
        let (tag, mono) = strip_mono(tag);
        let block = BLOCKS.contains(&name);
        nearest(&mut stack).pending = !block;
        if name != "span" {
            out.push_str(&tag);
        }
        last_br = name == "br";
        let in_code = stack.iter().any(|f| f.wrapped || f.name == "code" || f.name == "pre");
        let wrapped = mono && !NO_CODE.contains(&name) && !in_code;
        if wrapped {
            out.push_str("<code>");
        }
        if !VOID.contains(&name) {
            let kind = if block { Kind::Block } else { Kind::Inline };
            stack.push(Frame { name: name.into(), kind, wrapped, pending: false });
        }
    }
    trim_trailing_br(&mut out);
    out.trim().to_string()
}

/// Removes the transient mono marker from a start tag. Returns the cleaned tag and whether it was present.
fn strip_mono(tag: &str) -> (String, bool) {
    let mut tag = tag.to_string();
    let mut mono = false;
    if tag.contains(MONO_FACE) {
        tag = tag.replacen(MONO_FACE, "", 1);
        mono = true;
    }
    if let Some(s) = tag.find(" style=\"") {
        let v = s + 8;
        if let Some(len) = tag[v..].find('"') {
            let decls: Vec<&str> = tag[v..v + len].split(';').filter(|d| !d.trim().is_empty()).collect();
            let kept: Vec<&str> = decls.iter().copied().filter(|d| d.trim() != MONO_STYLE).collect();
            mono |= kept.len() != decls.len();
            let new = if kept.is_empty() { String::new() } else { format!(" style=\"{}\"", kept.join(";")) };
            tag.replace_range(s..v + len + 1, &new);
        }
    }
    (tag, mono)
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
        for gone in ["<div", "<span", "class=", "mso-", "font-family", "face=", "margin", "width="] {
            assert!(!out.html.contains(gone), "{gone} survived: {}", out.html);
        }
        assert_eq!(out.html.matches("<code>").count(), 3, "Consolas paragraph, Courier New font, mso span");
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
