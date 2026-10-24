//! In-house RTF tokenizer: MS-OXRTFEX HTML de-encapsulation (`\fromhtml1`) and plain-text extraction.
use encoding_rs::{Encoding, WINDOWS_1252};

use super::charset::for_codepage;

/// True when the RTF header (first 1,024 bytes) declares encapsulated HTML.
pub fn is_html_encapsulated(rtf: &[u8]) -> bool {
    rtf[..rtf.len().min(1024)].windows(10).any(|w| w == b"\\fromhtml1")
}

/// `\fromhtml1` RTF → the original HTML text.
pub fn deencapsulate_html(rtf: &[u8]) -> String { run(rtf, true) }

/// RTF without encapsulated HTML → plain text.
pub fn to_text(rtf: &[u8]) -> String { run(rtf, false) }

/// Groups whose content is never text.
const SKIP_DESTINATIONS: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "object", "header", "headerl", "headerr", "headerf",
    "footer", "footerl", "footerr", "footerf", "footnote", "listtable", "listoverridetable", "revtbl", "rsidtbl",
    "generator", "xmlnstbl", "themedata", "colorschememapping", "datastore", "latentstyles",
];

#[derive(Clone, Copy)]
struct GroupState { skip: bool, htmltag: bool, htmlrtf: bool, uc: usize }

struct Out {
    text: String,
    pending: Vec<u8>,
    enc: &'static Encoding,
}

impl Out {
    fn byte(&mut self, b: u8) { self.pending.push(b); }
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let (s, _) = self.enc.decode_without_bom_handling(&self.pending);
            self.text.push_str(&s);
            self.pending.clear();
        }
    }
    fn ch(&mut self, c: char) {
        self.flush();
        self.text.push(c);
    }
}

fn run(rtf: &[u8], html_mode: bool) -> String {
    let mut out = Out { text: String::new(), pending: Vec::new(), enc: WINDOWS_1252 };
    let mut stack: Vec<GroupState> = Vec::new();
    let mut st = GroupState { skip: false, htmltag: false, htmlrtf: false, uc: 1 };
    let mut skip_chars = 0usize; // fallback characters after a unicode escape still to skip
    let mut new_group = false; // the previous token was '{'
    let mut i = 0;
    let n = rtf.len();
    while i < n {
        let b = rtf[i];
        match b {
            b'{' => {
                stack.push(st);
                new_group = true;
                skip_chars = 0;
                i += 1;
                continue;
            }
            b'}' => {
                if let Some(prev) = stack.pop() {
                    st = prev;
                }
                skip_chars = 0;
                i += 1;
            }
            b'\r' | b'\n' => i += 1,
            b'\\' if i + 1 < n => {
                let c = rtf[i + 1];
                if c.is_ascii_alphabetic() {
                    let ws = i + 1;
                    let mut j = ws;
                    while j < n && rtf[j].is_ascii_alphabetic() { j += 1; }
                    let word = std::str::from_utf8(&rtf[ws..j]).unwrap_or("");
                    let ps = j;
                    if j < n && rtf[j] == b'-' { j += 1; }
                    while j < n && rtf[j].is_ascii_digit() { j += 1; }
                    let param: Option<i32> = if j > ps { std::str::from_utf8(&rtf[ps..j]).ok().and_then(|p| p.parse().ok()) } else { None };
                    if j < n && rtf[j] == b' ' { j += 1; }
                    i = j;
                    if word == "bin" {
                        i += param.unwrap_or(0).max(0) as usize;
                        new_group = false;
                        continue;
                    }
                    if new_group && SKIP_DESTINATIONS.contains(&word) {
                        st.skip = true;
                    }
                    control_word(word, param, &mut st, &mut out, &mut skip_chars, html_mode);
                } else if c == b'*' {
                    // {\*\dest …}: keep only \*\htmltag (HTML mode); skip every other optional destination.
                    let rest = &rtf[i + 2..];
                    let is_htmltag = rest.starts_with(b"\\htmltag");
                    if html_mode && is_htmltag {
                        st.htmltag = true;
                        st.skip = false;
                    } else {
                        st.skip = true;
                    }
                    i += 2;
                    continue; // keep new_group so "\*\htmltag" is still treated as the group's first word
                } else if c == b'\'' && i + 3 < n + 1 && i + 3 <= n {
                    let hex = std::str::from_utf8(&rtf[i + 2..(i + 4).min(n)]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                    i += 4;
                    if let Some(v) = hex {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else if emits(&st, html_mode) {
                            out.byte(v);
                        }
                    }
                } else {
                    i += 2;
                    let sym = match c {
                        b'\\' => Some('\\'),
                        b'{' => Some('{'),
                        b'}' => Some('}'),
                        b'~' => Some('\u{00A0}'),
                        b'_' => Some('-'),
                        b'\r' | b'\n' => Some('\n'),
                        _ => None,
                    };
                    if let Some(s) = sym {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else if emits(&st, html_mode) {
                            out.ch(s);
                        }
                    }
                }
            }
            _ => {
                i += 1;
                if skip_chars > 0 {
                    skip_chars -= 1;
                } else if emits(&st, html_mode) {
                    out.byte(b);
                }
            }
        }
        new_group = false;
    }
    out.flush();
    out.text
}

fn emits(st: &GroupState, html_mode: bool) -> bool {
    if st.skip {
        return false;
    }
    if html_mode { st.htmltag || !st.htmlrtf } else { true }
}

fn control_word(word: &str, param: Option<i32>, st: &mut GroupState, out: &mut Out, skip_chars: &mut usize, html_mode: bool) {
    match word {
        "ansicpg" => {
            out.flush();
            if let Some(enc) = param.and_then(|p| u32::try_from(p).ok()).and_then(for_codepage) {
                out.enc = enc;
            }
        }
        "htmltag" => {
            if html_mode {
                st.htmltag = true;
            }
        }
        "htmlrtf" => st.htmlrtf = param != Some(0),
        "uc" => st.uc = param.unwrap_or(1).max(0) as usize,
        "u" => {
            if let Some(p) = param {
                let cp = if p < 0 { (p + 65_536) as u32 } else { p as u32 };
                if emits(st, html_mode) {
                    out.ch(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                }
                *skip_chars = st.uc;
            }
        }
        "par" | "line" => {
            if emits(st, html_mode) {
                out.ch('\n');
            }
        }
        "tab" if emits(st, html_mode) => out.ch('\t'),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("missing golden file {}: {e}", p.display()))
    }

    #[test]
    fn fromhtml_golden() {
        let rtf = golden("fromhtml.rtf");
        assert!(is_html_encapsulated(&rtf));
        let expected = String::from_utf8(golden("fromhtml.expected.html")).unwrap();
        assert_eq!(deencapsulate_html(&rtf), expected.trim_end_matches('\n'));
    }

    #[test]
    fn plain_rtf_text() {
        let rtf = br"{\rtf1\ansi\ansicpg950\deff0{\fonttbl{\f0 PMingLiU;}}\f0 Hello\par \'b3\'f8\'bb\'f9\tab x\line y}";
        assert!(!is_html_encapsulated(rtf));
        assert_eq!(to_text(rtf), "Hello\n報價\tx\ny");
    }

    #[test]
    fn unicode_fallback_is_skipped() {
        // The control word is written as "\" + "u20729" so no editor turns it into a character.
        let rtf = concat!(r"{\rtf1\ansi\uc2 a\", "u20729", r"\'3f\'3fb\uc1\", "u21934", "?c}");
        assert_eq!(to_text(rtf.as_bytes()), "a\u{50F9}b\u{55AE}c");
    }
}
