use std::path::PathBuf;

use pff::{tags, File, Item};

fn fixture() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/external/outlook.pst");
    assert!(p.exists(), "fixture missing: {}", p.display());
    p
}

#[derive(Debug, PartialEq)]
struct Msg {
    folder: String,
    id: u32,
    class: Option<String>,
    subject: Option<String>,
    sender: Option<String>,
    has_html: bool,
    attachments: usize,
}

fn walk(item: &Item<'_>, path: &str, folders: &mut Vec<(String, u32)>, msgs: &mut Vec<Msg>) {
    folders.push((path.to_owned(), item.identifier().unwrap()));
    for i in 0..item.sub_message_count().unwrap() {
        let m = item.sub_message(i).unwrap();
        msgs.push(Msg {
            folder: path.to_owned(),
            id: m.identifier().unwrap(),
            class: m.string(0, tags::MESSAGE_CLASS).unwrap(),
            subject: m.string(0, tags::SUBJECT).unwrap(),
            sender: m.string(0, tags::SENDER_NAME).unwrap(),
            has_html: m.html_body().unwrap().is_some(),
            attachments: m.attachment_count().unwrap(),
        });
    }
    for i in 0..item.sub_folder_count().unwrap() {
        let f = item.sub_folder(i).unwrap();
        let name = f.folder_name().unwrap().unwrap_or_default();
        walk(&f, &format!("{path}/{name}"), folders, msgs);
    }
}

fn scan() -> (Vec<(String, u32)>, Vec<Msg>) {
    let file = File::open(&fixture()).unwrap();
    let root = file.root_folder().unwrap();
    let (mut folders, mut msgs) = (Vec::new(), Vec::new());
    walk(&root, "", &mut folders, &mut msgs);
    (folders, msgs)
}

#[test]
fn version_is_20260625() {
    assert_eq!(pff::version(), "20260625");
}

#[test]
fn non_pst_is_error() {
    let cargo_toml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    assert!(!pff::check_signature(&cargo_toml).unwrap());
    assert!(File::open(&cargo_toml).is_err());
    assert!(pff::check_signature(&fixture()).unwrap());
}

#[test]
fn outlook_pst_folders() {
    let (folders, _) = scan();
    let names: Vec<&str> = folders.iter().map(|(p, _)| p.rsplit('/').next().unwrap()).collect();
    assert!(names.contains(&"Inbox"), "folders: {folders:?}");
    assert!(names.contains(&"Sent Items"), "folders: {folders:?}");
    let file = File::open(&fixture()).unwrap();
    let store = file.message_store().unwrap();
    assert!(store.binary(0, 0x35e0).unwrap().is_some_and(|b| b.len() >= 4), "IPM subtree entry id present");
}

#[test]
fn outlook_pst_walk_is_deterministic() {
    let (f1, m1) = scan();
    let (f2, m2) = scan();
    assert_eq!(f1, f2);
    assert_eq!(m1, m2);
}

#[test]
fn outlook_pst_welcome_message() {
    let (_, msgs) = scan();
    let notes: Vec<&Msg> = msgs.iter().filter(|m| m.class.as_deref() == Some("IPM.Note")).collect();
    // The Welcome mail is stored twice: in Inbox and in the recovered folder Recovered_Group2.
    assert_eq!(notes.len(), 2, "messages: {msgs:?}");
    assert!(notes.iter().any(|m| m.folder.ends_with("/Inbox")), "{notes:?}");
    assert!(notes.iter().any(|m| m.folder.contains("Recovered_Group2")), "{notes:?}");
    for m in notes {
        // Real string properties: guards the libpff flags argument (0, not 0x01) of Item::string.
        assert_eq!(m.subject.as_deref(), Some("Welcome to Microsoft Office Outlook 2003"));
        assert_eq!(m.sender.as_deref(), Some("Outlook 2003 Team"));
        assert!(m.has_html);
        assert!(m.attachments >= 1);
    }
}

#[test]
fn filetime_conversion() {
    // 2024-05-01T08:30:00Z = 1714552200 s since the Unix epoch.
    let filetime = (1_714_552_200u64 + 11_644_473_600) * 10_000_000;
    assert_eq!(pff::filetime_to_unix_micros(filetime), Some(1_714_552_200_000_000));
    assert_eq!(pff::filetime_to_unix_micros(0), None);
}
