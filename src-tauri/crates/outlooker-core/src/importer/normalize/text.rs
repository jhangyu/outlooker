//! Part 04 §5.5 `body_text` and `snippet`.

pub const HTML2TEXT_WIDTH: usize = 1_000_000;

/// Search text of a sanitized HTML fragment (no decorations, table cells on separate lines, no wrapping).
pub fn html_to_text(sanitized_html: &str) -> Result<String, String> {
    html2text::config::plain_no_decorate()
        .raw_mode(true)
        .string_from_read(sanitized_html.as_bytes(), HTML2TEXT_WIDTH)
        .map_err(|e| e.to_string())
}

/// CRLF / CR → LF, runs of spaces / tabs → one space, 3+ LF → 2 LF, trim.
pub fn clean_text(s: &str) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(s.len());
    let mut lf_run = 0usize;
    let mut in_space = false;
    for c in s.chars() {
        match c {
            '\n' => {
                lf_run += 1;
                in_space = false;
                if lf_run <= 2 {
                    out.push('\n');
                }
            }
            ' ' | '\t' => {
                lf_run = 0;
                if !in_space {
                    out.push(' ');
                    in_space = true;
                }
            }
            _ => {
                lf_run = 0;
                in_space = false;
                out.push(c);
            }
        }
    }
    out.trim().to_string()
}

/// First 200 chars after every whitespace run → one space.
pub fn snippet(body_text: &str) -> String {
    body_text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(super::SNIPPET_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_and_snippet() {
        assert_eq!(clean_text("  a \t b\r\n\r\n\r\n\r\nc\rd  "), "a b\n\nc\nd");
        let long = "x ".repeat(300);
        assert_eq!(snippet(&long).chars().count(), 200);
        assert_eq!(snippet("a\n\n b\tc"), "a b c");
    }

    #[test]
    fn html_text_has_no_wrapping() {
        let words = "word ".repeat(100);
        let t = html_to_text(&format!("<p>{words}</p><table><tr><td>a</td><td>b</td></tr></table>")).unwrap();
        assert!(t.lines().next().unwrap().len() >= 400, "one long line: {t:?}");
        assert!(t.contains("a\nb") || t.contains("a\n\nb"), "cells on separate lines: {t:?}");
    }
}
