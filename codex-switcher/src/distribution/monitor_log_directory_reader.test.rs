use super::{clear_errno, MonitorLogDirectoryReader};
use std::fs::{self, File};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static READDIR_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn partial_listing_then_error(stream: *mut libc::DIR) -> *mut libc::dirent {
    if READDIR_CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        return unsafe { libc::readdir(stream) };
    }
    #[cfg(target_os = "macos")]
    unsafe {
        *libc::__error() = libc::EIO;
    }
    #[cfg(not(target_os = "macos"))]
    unsafe {
        *libc::__errno_location() = libc::EIO;
    }
    std::ptr::null_mut()
}

#[test]
fn readdir_error_after_a_partial_listing_fails_closed() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codex_monitor_directory_reader_{}_{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir(&path).unwrap();
    fs::write(path.join("owned.log.br"), "entry").unwrap();
    let directory = File::open(&path).unwrap();
    READDIR_CALLS.store(0, Ordering::SeqCst);
    clear_errno();

    let error =
        MonitorLogDirectoryReader::names_with_readdir(&directory, partial_listing_then_error)
            .unwrap_err();

    assert_eq!(error.raw_os_error(), Some(libc::EIO));
    fs::remove_dir_all(path).unwrap();
}
