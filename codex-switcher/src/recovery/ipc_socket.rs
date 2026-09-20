use crate::storage;
use std::os::{
    fd::AsRawFd,
    unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    },
};

pub(super) fn connect_verified_socket() -> Result<UnixStream, String> {
    let home = storage::codex_home();
    let directory = home.join("ipc");
    let socket = directory.join("ipc.sock");
    // SAFETY: geteuid has no preconditions and cannot mutate memory.
    let expected_uid = unsafe { libc::geteuid() };
    let directory_metadata = directory
        .symlink_metadata()
        .map_err(|error| format!("Codex IPC directory is unavailable: {error}"))?;
    let socket_metadata = socket
        .symlink_metadata()
        .map_err(|error| format!("Codex IPC socket is unavailable: {error}"))?;
    if !directory_metadata.is_dir()
        || directory_metadata.uid() != expected_uid
        || directory_metadata.mode() & 0o777 != 0o700
        || !socket_metadata.file_type().is_socket()
        || socket_metadata.uid() != expected_uid
        || socket_metadata.mode() & 0o777 != 0o600
    {
        return Err("Codex IPC ownership or permissions are unsafe".into());
    }
    let stream = UnixStream::connect(&socket).map_err(|error| error.to_string())?;
    let connected_metadata = socket
        .symlink_metadata()
        .map_err(|error| format!("Codex IPC socket changed during connection: {error}"))?;
    if !connected_metadata.file_type().is_socket()
        || connected_metadata.uid() != expected_uid
        || connected_metadata.mode() & 0o777 != 0o600
        || connected_metadata.dev() != socket_metadata.dev()
        || connected_metadata.ino() != socket_metadata.ino()
    {
        return Err("Codex IPC socket changed during connection".into());
    }
    let mut peer_uid = 0;
    let mut peer_gid = 0;
    // SAFETY: the descriptor is a live Unix stream and both output pointers
    // refer to initialized uid_t/gid_t values owned by this stack frame.
    let peer_result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut peer_uid, &mut peer_gid) };
    if peer_result != 0 || peer_uid != expected_uid {
        return Err("Codex IPC peer identity is unsafe".into());
    }
    Ok(stream)
}
