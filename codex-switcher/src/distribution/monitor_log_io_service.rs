use fs2::FileExt;
use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

pub struct MonitorLogIoService;

impl MonitorLogIoService {
    pub fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = Self::open_path(
            path,
            libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            PRIVATE_FILE_MODE,
            true,
        )?;
        file.lock_exclusive()?;
        let result = file.write_all(bytes);
        let _ = file.unlock();
        result
    }

    pub fn open_for_rotation(path: &Path) -> io::Result<Option<(File, File)>> {
        let active = match Self::open_path(
            path,
            libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            PRIVATE_FILE_MODE,
            false,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !active.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "active log is not a regular file",
            ));
        }

        let archive_path = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no parent"))?
            .join("archive");
        let archive = Self::open_directory_path(&archive_path, true)?;
        archive.set_permissions(fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))?;
        Ok(Some((active, archive)))
    }

    pub fn open_for_read(path: &Path) -> io::Result<Option<File>> {
        match Self::open_path(
            path,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            PRIVATE_FILE_MODE,
            false,
        ) {
            Ok(file) => {
                if !file.metadata()?.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "log is not a regular file",
                    ));
                }
                Ok(Some(file))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn create_archive_file(directory: &File, name: &str) -> io::Result<File> {
        let component = Self::component(name)?;
        let file = Self::open_at(
            directory,
            &component,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            PRIVATE_FILE_MODE,
        )?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive target is not a regular file",
            ));
        }
        file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
        Ok(file)
    }

    pub fn open_archive_file(directory: &File, name: &str) -> io::Result<File> {
        let component = Self::component(name)?;
        let file = Self::open_at(
            directory,
            &component,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive entry is not a regular file",
            ));
        }
        Ok(file)
    }

    pub fn open_child_file(parent: &File, name: &str, flags: i32) -> io::Result<File> {
        let component = Self::component(name)?;
        let file = Self::open_at(
            parent,
            &component,
            flags | libc::O_NOFOLLOW,
            PRIVATE_FILE_MODE,
        )?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log entry is not a regular file",
            ));
        }
        Ok(file)
    }

    pub fn archive_names(directory: &File) -> io::Result<Vec<String>> {
        let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            let error = io::Error::last_os_error();
            unsafe { libc::close(duplicate) };
            return Err(error);
        }

        let mut names = Vec::new();
        loop {
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                break;
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            let name = name.to_string_lossy();
            if name != "." && name != ".." {
                names.push(name.into_owned());
            }
        }
        let close_result = unsafe { libc::closedir(stream) };
        if close_result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(names)
    }

    pub fn remove_child(parent: &File, name: &str, directory: bool) -> io::Result<()> {
        let component = Self::component(name)?;
        let flags = if directory { libc::AT_REMOVEDIR } else { 0 };
        let result = unsafe { libc::unlinkat(parent.as_raw_fd(), component.as_ptr(), flags) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                return Ok(());
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn open_child_directory(
        parent: &File,
        name: &str,
        create_missing: bool,
    ) -> io::Result<Option<File>> {
        let component = Self::component(name)?;
        match Self::open_at(
            parent,
            &component,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        ) {
            Ok(directory) => Ok(Some(directory)),
            Err(error) if error.kind() == io::ErrorKind::NotFound && !create_missing => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_missing => {
                let result = unsafe {
                    libc::mkdirat(
                        parent.as_raw_fd(),
                        component.as_ptr(),
                        PRIVATE_DIRECTORY_MODE as libc::mode_t,
                    )
                };
                if result < 0 {
                    let mkdir_error = io::Error::last_os_error();
                    if mkdir_error.kind() != io::ErrorKind::AlreadyExists {
                        return Err(mkdir_error);
                    }
                }
                Self::open_at(
                    parent,
                    &component,
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0,
                )
                .map(Some)
            }
            Err(error) => Err(error),
        }
    }

    pub fn open_directory_path(path: &Path, create_missing: bool) -> io::Result<File> {
        let mut components = path.components();
        let mut current = match components.next() {
            Some(Component::RootDir) => Self::open_root_directory()?,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "log paths must be absolute",
                ))
            }
        };
        for component in components {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "log path contains an unsafe component",
                ));
            };
            let name = Self::component(name.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "log path is not UTF-8")
            })?)?;
            current = Self::open_child_directory(&current, name.to_str().unwrap(), create_missing)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "directory is missing"))?;
        }
        Ok(current)
    }

    pub fn open_path(path: &Path, flags: i32, mode: u32, create_parent: bool) -> io::Result<File> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no parent"))?;
        let parent = Self::open_directory_path(parent, create_parent)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid log filename"))?;
        let component = Self::component(name)?;
        Self::open_at(&parent, &component, flags, mode)
    }

    pub fn read_to_end(mut file: File) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn open_root_directory() -> io::Result<File> {
        Self::open_at_path(
            Path::new("/"),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    }

    fn open_at_path(path: &Path, flags: i32) -> io::Result<File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).custom_flags(flags);
        options.open(path)
    }

    fn open_at(parent: &File, component: &CString, flags: i32, mode: u32) -> io::Result<File> {
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                component.as_ptr(),
                flags,
                mode as libc::c_uint,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }

    fn component(name: &str) -> io::Result<CString> {
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsafe log path component",
            ));
        }
        CString::new(name)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in log path component"))
    }
}
