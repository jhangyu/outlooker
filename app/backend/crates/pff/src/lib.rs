//! Safe wrapper over the vendored libpff C library (spec Part 01 §4).
//! A `File` and its `Item`s belong to one thread: raw pointers make both `!Send` and `!Sync`.
//!
//! ```compile_fail
//! fn assert_send<T: Send>() {}
//! assert_send::<pff::File>();
//! ```

mod ffi;
pub mod tags;

use std::ffi::{c_char, c_int, CStr};
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Owns a `libpff_error_t` and frees it on drop.
struct ErrSlot(*mut ffi::libpff_error_t);

impl ErrSlot {
    fn new() -> ErrSlot {
        ErrSlot(ptr::null_mut())
    }
    fn ptr(&mut self) -> ffi::Err {
        &mut self.0
    }
    fn to_error(&self, context: &str) -> Error {
        if self.0.is_null() {
            return Error { message: format!("{context} failed") };
        }
        let mut buf = [0 as c_char; 2048];
        let rc = unsafe { ffi::libpff_error_sprint(self.0, buf.as_mut_ptr(), buf.len()) };
        let detail = if rc > 0 { unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned() } else { String::new() };
        Error { message: format!("{context}: {detail}") }
    }
}

impl Drop for ErrSlot {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ffi::libpff_error_free(&mut self.0) };
        }
    }
}

/// Runs one libpff call. 1 → Ok(true), 0 → Ok(false) (value not present), -1 → Err.
fn call(context: &str, f: impl FnOnce(ffi::Err) -> c_int) -> Result<bool> {
    let mut slot = ErrSlot::new();
    match f(slot.ptr()) {
        1 => Ok(true),
        0 => Ok(false),
        _ => Err(slot.to_error(context)),
    }
}

/// Size-then-read pattern; trailing NUL bytes are removed.
fn read_sized(
    context: &str,
    size_fn: impl FnOnce(*mut usize, ffi::Err) -> c_int,
    read_fn: impl FnOnce(*mut u8, usize, ffi::Err) -> c_int,
) -> Result<Option<Vec<u8>>> {
    let mut size = 0usize;
    if !call(context, |e| size_fn(&mut size, e))? || size == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; size];
    if !call(context, |e| read_fn(buf.as_mut_ptr(), size, e))? {
        return Ok(None);
    }
    while buf.last() == Some(&0) {
        buf.pop();
    }
    Ok(Some(buf))
}

fn read_string(
    context: &str,
    size_fn: impl FnOnce(*mut usize, ffi::Err) -> c_int,
    read_fn: impl FnOnce(*mut u8, usize, ffi::Err) -> c_int,
) -> Result<Option<String>> {
    Ok(read_sized(context, size_fn, read_fn)?.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

pub fn version() -> &'static str {
    unsafe { CStr::from_ptr(ffi::libpff_get_version()) }.to_str().unwrap_or("")
}

/// FILETIME (100 ns ticks since 1601-01-01) → Unix microseconds. 0 means "not set".
pub fn filetime_to_unix_micros(filetime: u64) -> Option<i64> {
    if filetime == 0 {
        return None;
    }
    Some((filetime / 10) as i64 - 11_644_473_600_000_000)
}

#[cfg(unix)]
fn narrow_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| Error { message: "path contains NUL".into() })
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub fn check_signature(path: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        let c = narrow_path(path)?;
        call("libpff_check_file_signature", |e| unsafe { ffi::libpff_check_file_signature(c.as_ptr(), e) })
    }
    #[cfg(windows)]
    {
        let w = wide_path(path);
        call("libpff_check_file_signature_wide", |e| unsafe { ffi::libpff_check_file_signature_wide(w.as_ptr(), e) })
    }
}

pub struct File {
    raw: *mut ffi::libpff_file_t,
    opened: bool,
}

impl File {
    pub fn open(path: &Path) -> Result<File> {
        let mut raw = ptr::null_mut();
        call("libpff_file_initialize", |e| unsafe { ffi::libpff_file_initialize(&mut raw, e) })?;
        let mut file = File { raw, opened: false };
        #[cfg(unix)]
        {
            let c = narrow_path(path)?;
            call("libpff_file_open", |e| unsafe { ffi::libpff_file_open(file.raw, c.as_ptr(), ffi::LIBPFF_OPEN_READ, e) })?;
        }
        #[cfg(windows)]
        {
            let w = wide_path(path);
            call("libpff_file_open_wide", |e| unsafe { ffi::libpff_file_open_wide(file.raw, w.as_ptr(), ffi::LIBPFF_OPEN_READ, e) })?;
        }
        file.opened = true;
        Ok(file)
    }

    pub fn root_folder(&self) -> Result<Item<'_>> {
        let mut raw = ptr::null_mut();
        if !call("libpff_file_get_root_folder", |e| unsafe { ffi::libpff_file_get_root_folder(self.raw, &mut raw, e) })? {
            return Err(Error { message: "PST has no root folder".into() });
        }
        Ok(Item::wrap(raw))
    }

    pub fn message_store(&self) -> Result<Item<'_>> {
        let mut raw = ptr::null_mut();
        if !call("libpff_file_get_message_store", |e| unsafe { ffi::libpff_file_get_message_store(self.raw, &mut raw, e) })? {
            return Err(Error { message: "PST has no message store".into() });
        }
        Ok(Item::wrap(raw))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if self.opened {
            let _ = call("libpff_file_close", |e| unsafe { ffi::libpff_file_close(self.raw, e) });
        }
        let _ = call("libpff_file_free", |e| unsafe { ffi::libpff_file_free(&mut self.raw, e) });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentType {
    Data,
    Item,
    Reference,
    Undefined,
}

pub struct Item<'f> {
    raw: *mut ffi::libpff_item_t,
    _file: PhantomData<&'f File>,
}

impl Drop for Item<'_> {
    fn drop(&mut self) {
        let _ = call("libpff_item_free", |e| unsafe { ffi::libpff_item_free(&mut self.raw, e) });
    }
}

impl<'f> Item<'f> {
    fn wrap(raw: *mut ffi::libpff_item_t) -> Item<'f> {
        Item { raw, _file: PhantomData }
    }

    fn child(&self, context: &str, f: impl FnOnce(*mut *mut ffi::libpff_item_t, ffi::Err) -> c_int) -> Result<Option<Item<'f>>> {
        let mut raw = ptr::null_mut();
        if call(context, |e| f(&mut raw, e))? && !raw.is_null() {
            Ok(Some(Item::wrap(raw)))
        } else {
            Ok(None)
        }
    }

    fn count(&self, context: &str, f: impl FnOnce(*mut c_int, ffi::Err) -> c_int) -> Result<usize> {
        let mut n: c_int = 0;
        call(context, |e| f(&mut n, e))?;
        Ok(n.max(0) as usize)
    }

    pub fn identifier(&self) -> Result<u32> {
        let mut id = 0u32;
        call("libpff_item_get_identifier", |e| unsafe { ffi::libpff_item_get_identifier(self.raw, &mut id, e) })?;
        Ok(id)
    }

    pub fn folder_name(&self) -> Result<Option<String>> {
        read_string(
            "libpff_folder_get_utf8_name",
            |s, e| unsafe { ffi::libpff_folder_get_utf8_name_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_folder_get_utf8_name(self.raw, b, n, e) },
        )
    }

    pub fn sub_folder_count(&self) -> Result<usize> {
        self.count("libpff_folder_get_number_of_sub_folders", |n, e| unsafe { ffi::libpff_folder_get_number_of_sub_folders(self.raw, n, e) })
    }

    pub fn sub_folder(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_folder_get_sub_folder", |r, e| unsafe { ffi::libpff_folder_get_sub_folder(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("sub folder {index} missing") })
    }

    pub fn sub_message_count(&self) -> Result<usize> {
        self.count("libpff_folder_get_number_of_sub_messages", |n, e| unsafe { ffi::libpff_folder_get_number_of_sub_messages(self.raw, n, e) })
    }

    pub fn sub_message(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_folder_get_sub_message", |r, e| unsafe { ffi::libpff_folder_get_sub_message(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("sub message {index} missing") })
    }

    pub fn record_set_count(&self) -> Result<usize> {
        self.count("libpff_item_get_number_of_record_sets", |n, e| unsafe { ffi::libpff_item_get_number_of_record_sets(self.raw, n, e) })
    }

    pub fn string(&self, set: usize, tag: u32) -> Result<Option<String>> {
        let set = set as c_int;
        read_string(
            "libpff_item_get_entry_value_utf8_string",
            |s, e| unsafe { ffi::libpff_item_get_entry_value_utf8_string_size(self.raw, set, tag, s, 0, e) },
            |b, n, e| unsafe { ffi::libpff_item_get_entry_value_utf8_string(self.raw, set, tag, b, n, 0, e) },
        )
    }

    pub fn u32_value(&self, set: usize, tag: u32) -> Result<Option<u32>> {
        let mut v = 0u32;
        let found = call("libpff_item_get_entry_value_32bit", |e| unsafe {
            ffi::libpff_item_get_entry_value_32bit(self.raw, set as c_int, tag, &mut v, 0, e)
        })?;
        Ok(found.then_some(v))
    }

    pub fn filetime(&self, set: usize, tag: u32) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_item_get_entry_value_filetime", |e| unsafe {
            ffi::libpff_item_get_entry_value_filetime(self.raw, set as c_int, tag, &mut v, 0, e)
        })?;
        Ok(found.then_some(v))
    }

    pub fn binary(&self, set: usize, tag: u32) -> Result<Option<Vec<u8>>> {
        let set = set as c_int;
        let mut size = 0usize;
        if !call("libpff_item_get_entry_value_binary_data_size", |e| unsafe {
            ffi::libpff_item_get_entry_value_binary_data_size(self.raw, set, tag, &mut size, 0, e)
        })? {
            return Ok(None);
        }
        let mut buf = vec![0u8; size];
        if size > 0 {
            call("libpff_item_get_entry_value_binary_data", |e| unsafe {
                ffi::libpff_item_get_entry_value_binary_data(self.raw, set, tag, buf.as_mut_ptr(), size, 0, e)
            })?;
        }
        Ok(Some(buf))
    }

    pub fn client_submit_time(&self) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_message_get_client_submit_time", |e| unsafe { ffi::libpff_message_get_client_submit_time(self.raw, &mut v, e) })?;
        Ok(found.then_some(v).filter(|&t| t != 0))
    }

    pub fn delivery_time(&self) -> Result<Option<u64>> {
        let mut v = 0u64;
        let found = call("libpff_message_get_delivery_time", |e| unsafe { ffi::libpff_message_get_delivery_time(self.raw, &mut v, e) })?;
        Ok(found.then_some(v).filter(|&t| t != 0))
    }

    pub fn plain_text_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_plain_text_body",
            |s, e| unsafe { ffi::libpff_message_get_plain_text_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_plain_text_body(self.raw, b, n, e) },
        )
    }

    pub fn rtf_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_rtf_body",
            |s, e| unsafe { ffi::libpff_message_get_rtf_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_rtf_body(self.raw, b, n, e) },
        )
    }

    pub fn html_body(&self) -> Result<Option<Vec<u8>>> {
        read_sized(
            "libpff_message_get_html_body",
            |s, e| unsafe { ffi::libpff_message_get_html_body_size(self.raw, s, e) },
            |b, n, e| unsafe { ffi::libpff_message_get_html_body(self.raw, b, n, e) },
        )
    }

    pub fn recipients(&self) -> Result<Option<Item<'f>>> {
        self.child("libpff_message_get_recipients", |r, e| unsafe { ffi::libpff_message_get_recipients(self.raw, r, e) })
    }

    pub fn attachment_count(&self) -> Result<usize> {
        self.count("libpff_message_get_number_of_attachments", |n, e| unsafe { ffi::libpff_message_get_number_of_attachments(self.raw, n, e) })
    }

    pub fn attachment(&self, index: usize) -> Result<Item<'f>> {
        self.child("libpff_message_get_attachment", |r, e| unsafe { ffi::libpff_message_get_attachment(self.raw, index as c_int, r, e) })?
            .ok_or_else(|| Error { message: format!("attachment {index} missing") })
    }

    pub fn attachment_type(&self) -> Result<AttachmentType> {
        let mut t: c_int = 0;
        call("libpff_attachment_get_type", |e| unsafe { ffi::libpff_attachment_get_type(self.raw, &mut t, e) })?;
        Ok(match t as u8 {
            b'd' => AttachmentType::Data,
            b'i' => AttachmentType::Item,
            b'r' => AttachmentType::Reference,
            _ => AttachmentType::Undefined,
        })
    }

    pub fn attachment_data_size(&self) -> Result<u64> {
        let mut size = 0u64;
        call("libpff_attachment_get_data_size", |e| unsafe { ffi::libpff_attachment_get_data_size(self.raw, &mut size, e) })?;
        Ok(size)
    }

    pub fn attachment_data(&self, max_bytes: usize) -> Result<Option<Vec<u8>>> {
        let size = self.attachment_data_size()?;
        if size > max_bytes as u64 {
            return Ok(None);
        }
        let size = size as usize;
        let mut slot = ErrSlot::new();
        if unsafe { ffi::libpff_attachment_data_seek_offset(self.raw, 0, ffi::SEEK_SET, slot.ptr()) } < 0 {
            return Err(slot.to_error("libpff_attachment_data_seek_offset"));
        }
        let mut buf = vec![0u8; size];
        let mut filled = 0usize;
        while filled < size {
            let mut slot = ErrSlot::new();
            let n = unsafe { ffi::libpff_attachment_data_read_buffer(self.raw, buf[filled..].as_mut_ptr(), size - filled, slot.ptr()) };
            if n < 0 {
                return Err(slot.to_error("libpff_attachment_data_read_buffer"));
            }
            if n == 0 {
                break;
            }
            filled += n as usize;
        }
        buf.truncate(filled);
        Ok(Some(buf))
    }

    pub fn attached_item(&self) -> Result<Option<Item<'f>>> {
        self.child("libpff_attachment_get_item", |r, e| unsafe { ffi::libpff_attachment_get_item(self.raw, r, e) })
    }
}
