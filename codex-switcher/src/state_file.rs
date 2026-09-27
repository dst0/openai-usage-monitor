//! Durable replacement of small private state documents such as the manual
//! and automatic reset journals. Each document is staged exclusively with
//! owner-only permissions, flushed, renamed over its final name, and its
//! directory is flushed so the rename survives power loss. Every filesystem
//! call goes through `StateFileOperations`, so tests can fail any step.

#[path = "state_file/private_state_file_write_service.rs"]
mod private_state_file_write_service;
#[path = "state_file/state_file_operations.rs"]
mod state_file_operations;
#[path = "state_file/state_file_write_failure.rs"]
mod state_file_write_failure;
#[path = "state_file/system_state_file_operations.rs"]
mod system_state_file_operations;

pub(crate) use private_state_file_write_service::PrivateStateFileWriteService;
pub(crate) use state_file_operations::StateFileOperations;
pub(crate) use state_file_write_failure::StateFileWriteFailure;
pub(crate) use system_state_file_operations::SystemStateFileOperations;

#[cfg(test)]
#[path = "state_file/fake_state_file_operations.test.rs"]
pub(crate) mod fake_state_file_operations;
