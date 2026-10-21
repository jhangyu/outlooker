//! Part 04 §5.5 HTML charset decode.
use encoding_rs::Encoding;

/// Windows code page number → encoding (Part 04 §5.5 rule 2). None = not in the table.
pub fn for_codepage(cp: u32) -> Option<&'static Encoding> {
    use encoding_rs::*;
    Some(match cp {
        65001 => UTF_8,
        1200 => UTF_16LE,
        1201 => UTF_16BE,
        936 => GBK,
        950 => BIG5,
        932 => SHIFT_JIS,
        949 => EUC_KR,
        1250 => WINDOWS_1250,
        1251 => WINDOWS_1251,
        1252 | 20127 | 28591 => WINDOWS_1252,
        1253 => WINDOWS_1253,
        1254 => WINDOWS_1254,
        1255 => WINDOWS_1255,
        1256 => WINDOWS_1256,
        1257 => WINDOWS_1257,
        1258 => WINDOWS_1258,
        28592 => ISO_8859_2,
        28593 => ISO_8859_3,
        28594 => ISO_8859_4,
        28595 => ISO_8859_5,
        28596 => ISO_8859_6,
        28597 => ISO_8859_7,
        28598 => ISO_8859_8,
        28599 => WINDOWS_1254, // ISO-8859-9 is windows-1254 in the WHATWG table
        28600 => ISO_8859_10,
        28603 => ISO_8859_13,
        28604 => ISO_8859_14,
        28605 => ISO_8859_15,
        51932 => EUC_JP,
        50220 => ISO_2022_JP,
        20866 => KOI8_R,
        874 => WINDOWS_874,
        _ => return None,
    })
}

/// `<meta charset=…>` / `http-equiv … charset=…` in the first 1,024 bytes.
pub fn sniff_meta(bytes: &[u8]) -> Option<&'static Encoding> {
    let head = &bytes[..bytes.len().min(1024)];
    let lower: Vec<u8> = head.iter().map(u8::to_ascii_lowercase).collect();
    let mut from = 0;
    while let Some(pos) = find(&lower[from..], b"<meta") {
        let start = from + pos;
        let end = find(&lower[start..], b">").map_or(lower.len(), |e| start + e);
        let tag = &lower[start..end];
        if let Some(c) = find(tag, b"charset") {
            let mut i = c + b"charset".len();
            while i < tag.len() && (tag[i] == b' ' || tag[i] == b'=') { i += 1; }
            while i < tag.len() && (tag[i] == b'"' || tag[i] == b'\'') { i += 1; }
            let label_start = i;
            while i < tag.len() && (tag[i].is_ascii_alphanumeric() || b"-_:.".contains(&tag[i])) { i += 1; }
            if let Some(enc) = Encoding::for_label(&tag[label_start..i]) {
                return Some(enc);
            }
        }
        from = end;
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Bytes → UTF-8 text. Never fails; invalid sequences become U+FFFD.
pub fn decode_html(bytes: &[u8], internet_codepage: Option<u32>) -> String {
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    let enc = internet_codepage
        .and_then(for_codepage)
        .or_else(|| sniff_meta(bytes))
        .unwrap_or_else(|| {
            let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
            det.feed(bytes, true);
            det.guess(None, chardetng::Utf8Detection::Allow)
        });
    enc.decode_without_bom_handling(bytes).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big5_and_gbk() {
        assert_eq!(decode_html(&[0xB3, 0xF8, 0xBB, 0xF9, 0xB3, 0xE6], Some(950)), "報價單");
        assert_eq!(decode_html(&[0xB1, 0xA8, 0xBC, 0xDB, 0xB5, 0xA5], Some(936)), "报价单");
    }

    #[test]
    fn meta_and_bom_rules() {
        let mut html = b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=big5\"></head><body>".to_vec();
        html.extend_from_slice(&[0xB3, 0xF8, 0xBB, 0xF9]);
        assert!(decode_html(&html, None).ends_with("報價"));
        assert!(decode_html(&html, Some(12345)).ends_with("報價"), "unknown codepage falls through to meta");
        let bom = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        assert_eq!(decode_html(&bom, Some(950)), "hi");
        assert_eq!(decode_html(&[b'a', 0xFF, b'b'], Some(65001)), "a\u{FFFD}b");
    }
}
