use super::monitor_log_io_service::MonitorLogIoService;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn repeated_directory_enumeration_starts_from_the_beginning() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codex_monitor_log_enumeration_{}_{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    let path = fs::canonicalize(path).unwrap();
    fs::write(path.join("first.log"), b"first").unwrap();
    fs::write(path.join("second.log"), b"second").unwrap();
    let directory = MonitorLogIoService::open_directory_path(&path, false).unwrap();

    let mut first = MonitorLogIoService::archive_names(&directory).unwrap();
    let mut second = MonitorLogIoService::archive_names(&directory).unwrap();
    first.sort();
    second.sort();

    assert_eq!(first, vec!["first.log", "second.log"]);
    assert_eq!(second, first);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn excessive_directory_cardinality_fails_closed() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codex_monitor_log_cardinality_{}_{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    let path = fs::canonicalize(path).unwrap();
    for index in 0..=4096 {
        fs::write(path.join(format!("entry-{index}")), b"").unwrap();
    }
    let directory = MonitorLogIoService::open_directory_path(&path, false).unwrap();

    assert!(MonitorLogIoService::archive_names(&directory).is_err());
    fs::remove_dir_all(path).unwrap();
}
