use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_DIRECTORY_NAME_BYTES: usize = 1024 * 1024;

pub(super) struct MonitorLogDirectoryReader;

impl MonitorLogDirectoryReader {
    pub(super) fn names(directory: &File) -> io::Result<Vec<String>> {
        Self::names_with_readdir(directory, libc::readdir)
    }

    fn names_with_readdir(
        directory: &File,
        readdir: unsafe extern "C" fn(*mut libc::DIR) -> *mut libc::dirent,
    ) -> io::Result<Vec<String>> {
        let current = CString::new(".").expect("static directory component");
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), current.as_ptr(), flags, 0) };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        let stream = unsafe { libc::fdopendir(descriptor) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            unsafe { libc::close(descriptor) };
            return Err(error);
        }

        let mut names = Vec::new();
        let mut name_bytes = 0usize;
        loop {
            clear_errno();
            let entry = unsafe { readdir(stream) };
            if entry.is_null() {
                let read_error = errno();
                let close_result = unsafe { libc::closedir(stream) };
                if read_error != 0 {
                    return Err(io::Error::from_raw_os_error(read_error));
                }
                if close_result != 0 {
                    return Err(io::Error::last_os_error());
                }
                return Ok(names);
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_string_lossy();
            if name == "." || name == ".." {
                continue;
            }
            name_bytes = name_bytes.saturating_add(name.len());
            if names.len() >= MAX_DIRECTORY_ENTRIES || name_bytes > MAX_DIRECTORY_NAME_BYTES {
                unsafe { libc::closedir(stream) };
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Monitor log directory exceeds safety bound",
                ));
            }
            names.push(name.into_owned());
        }
    }
}

#[cfg(target_os = "macos")]
fn errno() -> i32 {
    unsafe { *libc::__error() }
}

#[cfg(not(target_os = "macos"))]
fn errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn clear_errno() {
    unsafe { *libc::__error() = 0 }
}

#[cfg(not(target_os = "macos"))]
fn clear_errno() {
    unsafe { *libc::__errno_location() = 0 }
}

#[cfg(test)]
#[path = "monitor_log_directory_reader.test.rs"]
mod tests;
