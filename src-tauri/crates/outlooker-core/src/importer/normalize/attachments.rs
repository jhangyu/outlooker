//! Part 04 §5.8 attachment metadata.
use crate::importer::types::{AttachmentOut, RawAttachment};

const METHOD_EMBEDDED: i32 = 5;

fn cut(s: &str, max: usize) -> String { s.chars().take(max).collect() }

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

pub fn mime_from_extension(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "xml" => "application/xml",
        "json" => "application/json",
        "rtf" => "application/rtf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "gz" => "application/gzip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "msg" => "application/vnd.ms-outlook",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "wav" => "audio/wav",
        _ => return None,
    })
}

pub fn attachment_out(a: &RawAttachment) -> AttachmentOut {
    let method = a.attach_method.unwrap_or(0);
    let file_name = if method == METHOD_EMBEDDED {
        format!("{}.msg", non_empty(&a.embedded_subject).unwrap_or("message"))
    } else {
        non_empty(&a.long_filename)
            .or_else(|| non_empty(&a.filename))
            .or_else(|| non_empty(&a.display_name))
            .map(str::to_string)
            .unwrap_or_else(|| format!("attachment-{}", a.index))
    };
    let file_name = cut(&file_name, 1024);
    let size_bytes = a.data_size.or(a.attach_size).unwrap_or(0).max(0);
    let mime_type = if method == METHOD_EMBEDDED {
        "application/vnd.ms-outlook".to_string()
    } else {
        non_empty(&a.mime_tag)
            .map(|m| cut(m, 255))
            .or_else(|| mime_from_extension(&file_name).map(str::to_string))
            .unwrap_or_else(|| "application/octet-stream".to_string())
    };
    AttachmentOut { attachment_index: a.index, file_name, size_bytes, mime_type, attach_method: method }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_rules() {
        let embedded = attachment_out(&RawAttachment { index: 2, attach_method: Some(5), embedded_subject: Some("Re: plan".into()), attach_size: Some(-4), ..Default::default() });
        assert_eq!((embedded.file_name.as_str(), embedded.mime_type.as_str(), embedded.size_bytes), ("Re: plan.msg", "application/vnd.ms-outlook", 0));
        let guessed = attachment_out(&RawAttachment { index: 0, attach_method: Some(1), filename: Some("Q3.PDF".into()), data_size: Some(10), ..Default::default() });
        assert_eq!(guessed.mime_type, "application/pdf");
        let unnamed = attachment_out(&RawAttachment { index: 7, ..Default::default() });
        assert_eq!((unnamed.file_name.as_str(), unnamed.mime_type.as_str(), unnamed.attach_method), ("attachment-7", "application/octet-stream", 0));
        let long = attachment_out(&RawAttachment { long_filename: Some(format!("{}.txt", "n".repeat(2000))), ..Default::default() });
        assert_eq!(long.file_name.chars().count(), 1024);
    }
}
