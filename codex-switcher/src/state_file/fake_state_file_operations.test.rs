use super::state_file_operations::StateFileOperations;
use super::system_state_file_operations::SystemStateFileOperations;
use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

const OPERATIONS: [&str; 7] = [
    "prepare_directory",
    "create_staging",
    "save_staging",
    "replace",
    "sync_directory",
    "remove_staging",
    "open_for_read",
];

type Hook = Box<dyn FnOnce()>;

/// Real filesystem operations with scripted faults for one test's private
/// `CODEX_HOME`. A fault or hook names an operation and the 1-based count of
/// that operation's calls, so a test states exactly which write or read fails.
/// Every call is logged with its path (`save_staging` has none).
pub(crate) struct FakeStateFileOperations {
    faults: RefCell<Vec<(&'static str, usize)>>,
    hooks: RefCell<Vec<(&'static str, usize, Hook)>>,
    calls: RefCell<Vec<(&'static str, PathBuf)>>,
}

impl FakeStateFileOperations {
    pub(crate) fn new() -> Self {
        Self {
            faults: RefCell::new(Vec::new()),
            hooks: RefCell::new(Vec::new()),
            calls: RefCell::new(Vec::new()),
        }
    }

    /// Fails the `nth` call of `operation` with `EIO` without performing it.
    pub(crate) fn fail(self, operation: &'static str, nth: usize) -> Self {
        assert!(
            OPERATIONS.contains(&operation),
            "unknown operation {operation}"
        );
        self.faults.borrow_mut().push((operation, nth));
        self
    }

    /// Runs `hook` immediately before the `nth` call of `operation`, for
    /// example to model another writer replacing the file.
    pub(crate) fn before(
        self,
        operation: &'static str,
        nth: usize,
        hook: impl FnOnce() + 'static,
    ) -> Self {
        assert!(
            OPERATIONS.contains(&operation),
            "unknown operation {operation}"
        );
        self.hooks
            .borrow_mut()
            .push((operation, nth, Box::new(hook)));
        self
    }

    pub(crate) fn calls(&self) -> Vec<(&'static str, PathBuf)> {
        self.calls.borrow().clone()
    }

    pub(crate) fn operations(&self) -> Vec<&'static str> {
        self.calls.borrow().iter().map(|(name, _)| *name).collect()
    }

    /// Faults and hooks whose call never happened. A test asserts this is
    /// empty so a fault that no longer fires cannot pass vacuously.
    pub(crate) fn unfired(&self) -> Vec<(&'static str, usize)> {
        let count = |operation: &str| {
            self.calls
                .borrow()
                .iter()
                .filter(|(name, _)| *name == operation)
                .count()
        };
        let hooks = self.hooks.borrow();
        let hooks = hooks.iter().map(|(name, nth, _)| (*name, *nth));
        self.faults
            .borrow()
            .iter()
            .copied()
            .chain(hooks)
            .filter(|(name, nth)| count(name) < *nth)
            .collect()
    }

    fn enter(&self, operation: &'static str, path: &Path) -> io::Result<()> {
        let nth = self
            .calls
            .borrow()
            .iter()
            .filter(|(name, _)| *name == operation)
            .count()
            + 1;
        self.calls
            .borrow_mut()
            .push((operation, path.to_path_buf()));
        let hook = {
            let mut hooks = self.hooks.borrow_mut();
            hooks
                .iter()
                .position(|(name, at, _)| *name == operation && *at == nth)
                .map(|index| hooks.remove(index).2)
        };
        if let Some(hook) = hook {
            hook();
        }
        if self.faults.borrow().contains(&(operation, nth)) {
            return Err(io::Error::from_raw_os_error(libc::EIO));
        }
        Ok(())
    }
}

impl StateFileOperations for FakeStateFileOperations {
    fn prepare_directory(&self, directory: &Path) -> io::Result<()> {
        self.enter("prepare_directory", directory)?;
        SystemStateFileOperations.prepare_directory(directory)
    }

    fn create_staging(&self, staging: &Path) -> io::Result<File> {
        self.enter("create_staging", staging)?;
        SystemStateFileOperations.create_staging(staging)
    }

    fn save_staging(&self, file: &mut File, content: &[u8]) -> io::Result<()> {
        self.enter("save_staging", Path::new(""))?;
        SystemStateFileOperations.save_staging(file, content)
    }

    fn replace(&self, staging: &Path, path: &Path) -> io::Result<()> {
        self.enter("replace", path)?;
        SystemStateFileOperations.replace(staging, path)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        self.enter("sync_directory", directory)?;
        SystemStateFileOperations.sync_directory(directory)
    }

    fn remove_staging(&self, staging: &Path) -> io::Result<()> {
        self.enter("remove_staging", staging)?;
        SystemStateFileOperations.remove_staging(staging)
    }

    fn open_for_read(&self, path: &Path) -> io::Result<File> {
        self.enter("open_for_read", path)?;
        SystemStateFileOperations.open_for_read(path)
    }
}
