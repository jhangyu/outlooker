//! `outlooker-loadgen seed --data-root <dir> --messages <n>`
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: outlooker-loadgen seed --data-root <dir> --messages <n>";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut root, mut messages) = (None, None);
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data-root" => root = it.next().map(PathBuf::from),
            "--messages" => messages = it.next().and_then(|v| v.parse::<u64>().ok()),
            _ => messages = None,
        }
    }
    let (Some("seed"), Some(root), Some(messages)) = (args.first().map(String::as_str), root, messages) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match outlooker_loadgen::load::seed(&root, messages).await {
        Ok(r) => {
            println!("messages_stored={} occurrences={}", r.messages_stored, r.occurrences);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("seed failed: {e}");
            ExitCode::FAILURE
        }
    }
}
