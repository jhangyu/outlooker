#![allow(non_camel_case_types)]
use std::ffi::{c_char, c_int};

#[repr(C)]
pub struct libpff_error_t {
    _private: [u8; 0],
}
#[repr(C)]
pub struct libpff_file_t {
    _private: [u8; 0],
}
#[repr(C)]
pub struct libpff_item_t {
    _private: [u8; 0],
}

pub type Err = *mut *mut libpff_error_t;
pub const LIBPFF_OPEN_READ: c_int = 0x01;
pub const SEEK_SET: c_int = 0;

unsafe extern "C" {
    pub fn libpff_get_version() -> *const c_char;
    pub fn libpff_error_free(error: *mut *mut libpff_error_t);
    pub fn libpff_error_sprint(error: *mut libpff_error_t, string: *mut c_char, size: usize) -> c_int;
    pub fn libpff_check_file_signature(filename: *const c_char, error: Err) -> c_int;
    #[cfg(windows)]
    pub fn libpff_check_file_signature_wide(filename: *const u16, error: Err) -> c_int;
    pub fn libpff_file_initialize(file: *mut *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_free(file: *mut *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_open(file: *mut libpff_file_t, filename: *const c_char, access_flags: c_int, error: Err) -> c_int;
    #[cfg(windows)]
    pub fn libpff_file_open_wide(file: *mut libpff_file_t, filename: *const u16, access_flags: c_int, error: Err) -> c_int;
    pub fn libpff_file_close(file: *mut libpff_file_t, error: Err) -> c_int;
    pub fn libpff_file_get_root_folder(file: *mut libpff_file_t, root: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_file_get_message_store(file: *mut libpff_file_t, store: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_attachment_get_item(attachment: *mut libpff_item_t, item: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_item_free(item: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_item_get_identifier(item: *mut libpff_item_t, identifier: *mut u32, error: Err) -> c_int;
    pub fn libpff_item_get_number_of_record_sets(item: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_utf8_string_size(item: *mut libpff_item_t, set: c_int, entry_type: u32, size: *mut usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_utf8_string(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u8, size: usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_32bit(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u32, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_filetime(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u64, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_binary_data_size(item: *mut libpff_item_t, set: c_int, entry_type: u32, size: *mut usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_item_get_entry_value_binary_data(item: *mut libpff_item_t, set: c_int, entry_type: u32, value: *mut u8, size: usize, flags: u8, error: Err) -> c_int;
    pub fn libpff_folder_get_utf8_name_size(folder: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_folder_get_utf8_name(folder: *mut libpff_item_t, value: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_folder_get_number_of_sub_folders(folder: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_folder_get_sub_folder(folder: *mut libpff_item_t, index: c_int, sub: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_folder_get_number_of_sub_messages(folder: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_folder_get_sub_message(folder: *mut libpff_item_t, index: c_int, sub: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_message_get_client_submit_time(message: *mut libpff_item_t, filetime: *mut u64, error: Err) -> c_int;
    pub fn libpff_message_get_delivery_time(message: *mut libpff_item_t, filetime: *mut u64, error: Err) -> c_int;
    pub fn libpff_message_get_plain_text_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_plain_text_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_rtf_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_rtf_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_html_body_size(message: *mut libpff_item_t, size: *mut usize, error: Err) -> c_int;
    pub fn libpff_message_get_html_body(message: *mut libpff_item_t, body: *mut u8, size: usize, error: Err) -> c_int;
    pub fn libpff_message_get_recipients(message: *mut libpff_item_t, recipients: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_message_get_number_of_attachments(message: *mut libpff_item_t, count: *mut c_int, error: Err) -> c_int;
    pub fn libpff_message_get_attachment(message: *mut libpff_item_t, index: c_int, attachment: *mut *mut libpff_item_t, error: Err) -> c_int;
    pub fn libpff_attachment_get_type(attachment: *mut libpff_item_t, kind: *mut c_int, error: Err) -> c_int;
    pub fn libpff_attachment_get_data_size(attachment: *mut libpff_item_t, size: *mut u64, error: Err) -> c_int;
    pub fn libpff_attachment_data_read_buffer(attachment: *mut libpff_item_t, buffer: *mut u8, size: usize, error: Err) -> isize;
    pub fn libpff_attachment_data_seek_offset(attachment: *mut libpff_item_t, offset: i64, whence: c_int, error: Err) -> i64;
}
