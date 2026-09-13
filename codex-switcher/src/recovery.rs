//! Recovery is successful only when the target rollout records new agent work.
//! A deep link, AXPress, task_started, or a queue acknowledgement is not proof.
use crate::{storage, switcher};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const IPC_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const IPC_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(30);
const IPC_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const IPC_OWNER_TIMEOUT: Duration = Duration::from_secs(30);
// Current Desktop routers allow each registered client 10 seconds to answer a
// discovery probe. The forwarded request timeout starts only after a client is
// selected, so our transport deadline must cover both bounded phases.
const IPC_ROUTER_DISCOVERY_BUDGET: Duration = Duration::from_secs(10);
const IPC_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(12);
// The Desktop router applies timeoutMs before it writes its no-client-found
// response. Keep the local socket alive slightly longer so the router's
// terminal response cannot race our read timeout.
const IPC_RESPONSE_GRACE: Duration = Duration::from_secs(2);
const MAX_LINE: usize = 131072;
const MAX_IPC_FRAME: usize = 2 * 1024 * 1024;
const MAX_QUEUE_STATE: usize = 512 * 1024;
const SQLITE_READ_ATTEMPTS: usize = 6;
const SQLITE_BUSY_TIMEOUT_MS: u64 = 3000;
const MIN_BANNER_VISIBLE: Duration = Duration::from_secs(30);
const RECOVERY_DISPATCH_TIMEOUT: Duration = Duration::from_secs(180);
const RECOVERY_EXECUTION_TIMEOUT: Duration = Duration::from_secs(600);
const PRE_DISPATCH_ACTIVITY_GRACE: Duration = Duration::from_secs(3);
const INTERRUPTED_QUEUE_PAUSE: &str = "Interrupted before the steer was accepted.";
pub(crate) const AUTOMATION_COOLDOWN: Duration = Duration::from_secs(180);
const DESKTOP_STABILITY_WINDOW: Duration = Duration::from_secs(90);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryMode {
    CapturedRestart,
    ExplicitTarget,
    DiscoveredOnly,
}

impl RecoveryMode {
    fn captured(self) -> bool {
        self == Self::CapturedRestart
    }

    fn allows_ambiguous_active_dispatch(self) -> bool {
        matches!(self, Self::CapturedRestart | Self::ExplicitTarget)
    }
}

pub(crate) struct RecoveryBanner {
    child: Option<Child>,
    visible_since: Option<Instant>,
}

impl RecoveryBanner {
    pub(crate) fn start(task_count: usize) -> Result<Self, String> {
        if task_count == 0 {
            return Ok(Self {
                child: None,
                visible_since: None,
            });
        }
        let directory = storage::codex_home().join("recovery-runs");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let ready_path = directory.join(format!("banner-{}.ready", std::process::id()));
        let _ = std::fs::remove_file(&ready_path);
        for path in helper_candidates().into_iter().flatten() {
            if !path.exists() {
                continue;
            }
            match Command::new(path)
                .args([
                    "--automation-banner",
                    "--tasks",
                    &task_count.to_string(),
                    "--parent-pid",
                    &std::process::id().to_string(),
                    "--ready-file",
                    &ready_path.to_string_lossy(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        if matches!(
                            std::fs::read(&ready_path),
                            Ok(ref bytes) if bytes == b"visible\n"
                        ) {
                            let _ = std::fs::remove_file(&ready_path);
                            println!("RECOVERY_BANNER_CONFIRMED tasks={task_count}");
                            return Ok(Self {
                                child: Some(child),
                                visible_since: Some(Instant::now()),
                            });
                        }
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        sleep(Duration::from_millis(50));
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&ready_path);
                }
                Err(_) => continue,
            }
        }
        Err("Automation banner could not create visible panels; refusing to restart Codex".into())
    }
}

impl Drop for RecoveryBanner {
    fn drop(&mut self) {
        if let Some(visible_since) = self.visible_since {
            let elapsed = visible_since.elapsed();
            if elapsed < MIN_BANNER_VISIBLE {
                sleep(MIN_BANNER_VISIBLE - elapsed);
            }
        }
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn operation_lock() -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(storage::codex_home().join("desktop-recovery.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "Another desktop switch/recovery is in progress".to_string())?;
    Ok(file)
}

fn cooldown_path(home: &Path) -> PathBuf {
    home.join("desktop-automation-cooldown")
}

fn cooldown_deadline_ms(now: SystemTime, duration: Duration) -> Result<u64, String> {
    let deadline = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before Unix epoch".to_string())?
        .checked_add(duration)
        .ok_or("Automation cooldown deadline overflow")?;
    u64::try_from(deadline.as_millis()).map_err(|_| "Automation cooldown deadline overflow".into())
}

fn arm_automation_cooldown_at(
    home: &Path,
    now: SystemTime,
    duration: Duration,
) -> Result<(), String> {
    std::fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let deadline = cooldown_deadline_ms(now, duration)?;
    let unique = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = home.join(format!(
        "desktop-automation-cooldown.{}.{unique}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        writeln!(file, "{deadline}").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, cooldown_path(home)).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Suppresses every automatic account switch for a short, durable settling
/// period. This tiny atomically-replaced state is intentionally not Brotli-
/// compressed because it needs cheap random reads on every daemon tick.
pub(crate) fn arm_automation_cooldown() -> Result<(), String> {
    arm_automation_cooldown_at(
        &storage::codex_home(),
        SystemTime::now(),
        AUTOMATION_COOLDOWN,
    )
}

fn automation_cooldown_remaining_at(
    home: &Path,
    now: SystemTime,
) -> Result<Option<Duration>, String> {
    let value = match std::fs::read_to_string(cooldown_path(home)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let deadline_ms = value
        .trim()
        .parse::<u64>()
        .map_err(|_| "Invalid desktop automation cooldown state".to_string())?;
    let now_ms = u64::try_from(
        now.duration_since(UNIX_EPOCH)
            .map_err(|_| "System clock is before Unix epoch".to_string())?
            .as_millis(),
    )
    .map_err(|_| "System clock overflow".to_string())?;
    Ok((deadline_ms > now_ms).then(|| Duration::from_millis(deadline_ms - now_ms)))
}

pub(crate) fn automation_cooldown_remaining() -> Result<Option<Duration>, String> {
    automation_cooldown_remaining_at(&storage::codex_home(), SystemTime::now())
}

fn restart_cancellation_path() -> PathBuf {
    storage::codex_home()
        .join("recovery-runs")
        .join("cancel-restart")
}

pub(crate) fn restart_cancellation_requested() -> bool {
    restart_cancellation_path().is_file()
}

pub(crate) fn clear_restart_cancellation() -> Result<(), String> {
    match std::fs::remove_file(restart_cancellation_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn helper_candidates() -> [Option<PathBuf>; 2] {
    [
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|dir| dir.join("codex-ui-resume"))),
        dirs::home_dir().map(|home| home.join(".local/bin/codex-ui-resume")),
    ]
}

pub(crate) fn activate_and_verify_desktop(expected_pid: u32) -> Result<(), String> {
    for helper in helper_candidates().into_iter().flatten() {
        if !helper.exists() {
            continue;
        }
        let output = Command::new(helper)
            .args([
                "--verify-visible",
                "--expected-pid",
                &expected_pid.to_string(),
            ])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("APP_VISIBLE")
        {
            println!("RESTART_VISIBLE pid={expected_pid}");
            return Ok(());
        }
    }
    Err(format!(
        "Codex process {expected_pid} has no verified visible, non-minimized window"
    ))
}

pub(crate) fn verify_desktop_stable(expected_pids: &[u32]) -> Result<(), String> {
    if expected_pids.len() != 1 {
        return Err(format!(
            "Codex launch must produce exactly one main process, got {expected_pids:?}"
        ));
    }
    let deadline = Instant::now() + DESKTOP_STABILITY_WINDOW;
    loop {
        if switcher::current_codex_app_pids() != expected_pids {
            return Err(
                "Codex main process changed or exited during recovery stabilization".into(),
            );
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(250));
    }
    activate_and_verify_desktop(expected_pids[0])?;
    println!(
        "RESTART_STABLE pids={expected_pids:?} observation_secs={}",
        DESKTOP_STABILITY_WINDOW.as_secs()
    );
    Ok(())
}

/// Desktop's same-user IPC router is the supported cross-client ownership
/// channel. Requests are routed to the window that owns the mounted thread,
/// and that window starts the turn on its existing app-server connection.
struct DesktopIpc {
    stream: UnixStream,
    client_id: String,
    next_id: u64,
}

#[derive(Debug)]
enum IpcCallError {
    NoClientFound,
    Other(String),
}

#[derive(Debug)]
enum IpcReadError {
    Io(std::io::Error),
    Protocol(String),
}

#[derive(Debug, Clone)]
struct OwnerInfo {
    client_id: String,
}

impl std::fmt::Display for IpcReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Protocol(error) => formatter.write_str(error),
        }
    }
}

fn ipc_response_wait(method: &str, timeout: Duration) -> Duration {
    timeout
        .saturating_add(if method == "initialize" {
            Duration::ZERO
        } else {
            IPC_ROUTER_DISCOVERY_BUDGET
        })
        .saturating_add(IPC_RESPONSE_GRACE)
}

impl std::fmt::Display for IpcCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClientFound => formatter.write_str("no-client-found"),
            Self::Other(error) => formatter.write_str(error),
        }
    }
}

impl DesktopIpc {
    fn connect_with_retry(timeout: Duration) -> Result<Self, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let last_error = match Self::connect(Duration::from_secs(5)) {
                Ok(client) => return Ok(client),
                Err(error) => error,
            };
            if Instant::now() >= deadline {
                return Err(format!(
                    "Codex Desktop IPC did not become ready within {}s ({})",
                    timeout.as_secs(),
                    last_error
                ));
            }
            sleep(Duration::from_millis(200));
        }
    }

    fn connect(timeout: Duration) -> Result<Self, String> {
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
        let peer_result =
            unsafe { libc::getpeereid(stream.as_raw_fd(), &mut peer_uid, &mut peer_gid) };
        if peer_result != 0 || peer_uid != expected_uid {
            return Err("Codex IPC peer identity is unsafe".into());
        }
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        let mut client = Self {
            stream,
            client_id: "initializing-client".into(),
            next_id: 1,
        };
        let initialized = client
            .request_raw(
                "initialize",
                0,
                serde_json::json!({ "clientType": "codex-monitor" }),
                None,
                timeout,
            )
            .map_err(|error| error.to_string())?;
        client.client_id = initialized["result"]["clientId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Codex IPC initialize returned no client ID")?
            .to_string();
        Ok(client)
    }

    fn request_raw(
        &mut self,
        method: &str,
        version: u64,
        params: Value,
        target_client_id: Option<&str>,
        timeout: Duration,
    ) -> Result<Value, IpcCallError> {
        let deadline = Instant::now() + ipc_response_wait(method, timeout);
        let request_id = format!(
            "codex-monitor-{}-{}-{}",
            std::process::id(),
            self.next_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.next_id += 1;
        let mut request = serde_json::json!({
            "type": "request",
            "requestId": request_id,
            "sourceClientId": self.client_id,
            "version": version,
            "method": method,
            "params": params,
            "timeoutMs": timeout.as_millis().min(u64::MAX as u128) as u64
        });
        if let Some(target) = target_client_id {
            request["targetClientId"] = Value::String(target.to_string());
        }
        write_ipc_frame(&mut self.stream, &request).map_err(IpcCallError::Other)?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(IpcCallError::Other(format!(
                    "Codex IPC request {method} timed out"
                )));
            }
            self.stream
                .set_read_timeout(Some(remaining))
                .map_err(|error| IpcCallError::Other(error.to_string()))?;
            let response = match read_ipc_frame(&mut self.stream) {
                Ok(response) => response,
                Err(IpcReadError::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(IpcCallError::Other(format!(
                        "Codex IPC request {method} timed out"
                    )));
                }
                Err(error) => return Err(IpcCallError::Other(error.to_string())),
            };
            if response["type"].as_str() != Some("response")
                || response["requestId"].as_str() != Some(&request_id)
            {
                continue;
            }
            if response["resultType"].as_str() == Some("error") {
                let error = response["error"].as_str().unwrap_or("unknown error");
                return Err(if error == "no-client-found" {
                    IpcCallError::NoClientFound
                } else {
                    IpcCallError::Other(format!("Codex IPC rejected {method}: {error}"))
                });
            }
            if response["resultType"].as_str() != Some("success") {
                return Err(IpcCallError::Other(format!(
                    "Codex IPC returned an invalid response for {method}"
                )));
            }
            return Ok(response);
        }
    }

    fn discover_owner_info_once(&mut self, thread_id: &str) -> Result<OwnerInfo, IpcCallError> {
        let response = self.request_raw(
            "thread-owner-discovery",
            1,
            serde_json::json!({ "hostId": "local", "conversationId": thread_id }),
            None,
            IPC_DISCOVERY_TIMEOUT,
        )?;
        parse_owner_info(&response).map_err(IpcCallError::Other)
    }

    fn discover_owner_info_with_retry(&mut self, thread_id: &str) -> Result<OwnerInfo, String> {
        let deadline = Instant::now() + IPC_OWNER_TIMEOUT;
        loop {
            let last_error = match self.discover_owner_info_once(thread_id) {
                Ok(owner) => return Ok(owner),
                Err(IpcCallError::NoClientFound) => "no-client-found".to_string(),
                Err(error) => return Err(error.to_string()),
            };
            if Instant::now() >= deadline {
                return Err(format!(
                    "Codex Desktop did not mount the thread within {}s ({})",
                    IPC_OWNER_TIMEOUT.as_secs(),
                    last_error
                ));
            }
            sleep(Duration::from_millis(200));
        }
    }

    fn ensure_thread_owner(&mut self, thread_id: &str) -> Result<(String, bool), String> {
        let (owner, mounted_by_recovery) = self.ensure_thread_owner_info(thread_id)?;
        Ok((owner.client_id, mounted_by_recovery))
    }

    fn ensure_thread_owner_info(&mut self, thread_id: &str) -> Result<(OwnerInfo, bool), String> {
        // An already visible task may have an owner immediately. A cold task
        // has none until its deep link mounts the Desktop view asynchronously.
        let (owner, mounted_by_recovery) = match self.discover_owner_info_once(thread_id) {
            Ok(owner) => (owner, false),
            Err(IpcCallError::NoClientFound) => {
                switcher::open_thread_in_codex(thread_id);
                (self.discover_owner_info_with_retry(thread_id)?, true)
            }
            Err(error) => return Err(error.to_string()),
        };
        Ok((owner, mounted_by_recovery))
    }

    fn resume_interrupted_turn(&mut self, thread_id: &str) -> Result<(bool, String), String> {
        let (owner, mounted_by_recovery) = self.ensure_thread_owner(thread_id)?;
        let response = self
            .request_raw(
                "thread-follower-start-turn",
                2,
                recovery_turn_start_request(thread_id),
                Some(&owner),
                IPC_CALL_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        let turn_id = validate_start_response(&response, &owner)?;
        Ok((mounted_by_recovery, turn_id))
    }

    fn resume_existing_queue(
        &mut self,
        thread_id: &str,
        messages: Vec<Value>,
    ) -> Result<bool, String> {
        let (owner, mounted_by_recovery) = self.ensure_thread_owner(thread_id)?;
        let mut state = serde_json::Map::new();
        state.insert(thread_id.to_string(), Value::Array(messages));
        let response = self
            .request_raw(
                "thread-follower-set-queued-follow-ups-state",
                1,
                serde_json::json!({
                    "conversationId": thread_id,
                    "state": state
                }),
                Some(&owner),
                IPC_CALL_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        if response["method"].as_str() != Some("thread-follower-set-queued-follow-ups-state")
            || response["handledByClientId"].as_str() != Some(owner.as_str())
            || response["result"]["ok"].as_bool() != Some(true)
        {
            return Err("Codex Desktop did not confirm queue recovery".into());
        }
        Ok(mounted_by_recovery)
    }
}

fn recovery_turn_start_request(thread_id: &str) -> Value {
    serde_json::json!({
        "conversationId": thread_id,
        "turnStart": {
            "request": {
                "threadId": thread_id,
                "input": [{
                    "type": "text",
                    "text": "continue",
                    "text_elements": []
                }],
                "turnTrigger": "app_update_resume"
            },
            "context": { "inheritThreadSettings": true }
        }
    })
}

fn parse_owner_info(response: &Value) -> Result<OwnerInfo, String> {
    if response["method"].as_str() != Some("thread-owner-discovery")
        || response["result"]["supportsUntrustedAppInput"].as_bool() != Some(true)
    {
        return Err("The Codex thread owner does not accept local IPC input".into());
    }
    let client_id = response["handledByClientId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Codex Desktop did not report a thread owner".to_string())?;
    Ok(OwnerInfo { client_id })
}

fn validate_start_response(response: &Value, owner: &str) -> Result<String, String> {
    if response["method"].as_str() != Some("thread-follower-start-turn") {
        return Err("Codex Desktop returned a mismatched recovery response".into());
    }
    if response["handledByClientId"].as_str() != Some(owner) {
        return Err("Codex Desktop returned recovery from an unexpected owner".into());
    }
    response["result"]["result"]["turn"]["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .map(str::to_string)
        .ok_or_else(|| "Codex Desktop did not return a valid started turn ID".into())
}

fn write_ipc_frame(stream: &mut UnixStream, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if payload.is_empty() || payload.len() > MAX_IPC_FRAME {
        return Err("Codex IPC request exceeds the recovery memory limit".into());
    }
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .and_then(|()| stream.write_all(&payload))
        .and_then(|()| stream.flush())
        .map_err(|error| error.to_string())
}

fn read_ipc_frame(stream: &mut UnixStream) -> Result<Value, IpcReadError> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length).map_err(IpcReadError::Io)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_IPC_FRAME {
        return Err(IpcReadError::Protocol(
            "Codex IPC response exceeds the recovery memory limit".into(),
        ));
    }
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload).map_err(IpcReadError::Io)?;
    serde_json::from_slice(&payload).map_err(|error| IpcReadError::Protocol(error.to_string()))
}

/// Fail closed before stopping Codex by handshaking its same-user IPC router.
pub(crate) fn preflight_desktop_dispatch() -> Result<(), String> {
    let client = DesktopIpc::connect_with_retry(IPC_PREFLIGHT_TIMEOUT)?;
    drop(client);
    println!("RECOVERY_CHANNEL_CONFIRMED transport=desktop_ipc");
    Ok(())
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn valid_operation_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_digit() || byte == b'-')
}

fn claim_restart_operation_at(home: &Path, operation_id: &str) -> Result<bool, String> {
    if !valid_operation_id(operation_id) {
        return Err("Invalid restart operation ID".into());
    }
    let directory = home.join("recovery-runs");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("restart-{operation_id}.claimed"));
    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(b"claimed\n")
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Destructive restart workers are at-most-once. `launchctl submit` keeps a
/// failed job alive, so an operation tombstone must be claimed before the app
/// is stopped. A relaunched worker sees the claim and exits without restarting.
pub fn claim_restart_operation() -> Result<bool, String> {
    let operation_id = std::env::var("CODEX_RESTART_OPERATION")
        .map_err(|_| "Restart worker is missing its operation ID".to_string())?;
    claim_restart_operation_at(&storage::codex_home(), &operation_id)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PendingTarget {
    id: String,
    offset: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PendingManifest {
    version: u8,
    targets: Vec<PendingTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StoredManifest {
    Current(PendingManifest),
    Legacy(Vec<String>),
}

fn load_manifest() -> Result<Vec<PendingTarget>, String> {
    let path = storage::codex_home().join("desktop-recovery.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            let stored: StoredManifest =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let targets = match stored {
                StoredManifest::Current(manifest) if manifest.version == 1 => manifest.targets,
                StoredManifest::Current(_) => {
                    return Err("Unsupported recovery manifest version".into())
                }
                StoredManifest::Legacy(ids) => ids
                    .into_iter()
                    .map(|id| PendingTarget { id, offset: None })
                    .collect(),
            };
            if targets.iter().all(|target| valid_id(&target.id)) {
                Ok(targets)
            } else {
                Err("Invalid recovery manifest".into())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

pub fn load_pending() -> Result<Vec<String>, String> {
    Ok(load_manifest()?
        .into_iter()
        .map(|target| target.id)
        .collect())
}

fn write_manifest(targets: &[PendingTarget]) -> Result<(), String> {
    if !targets.iter().all(|target| valid_id(&target.id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let tmp = home.join(format!("desktop-recovery.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let manifest = PendingManifest {
        version: 1,
        targets: targets.to_vec(),
    };
    let result = (|| {
        file.write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, home.join("desktop-recovery.json")).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Small atomic restart journal. Contains UUIDs and byte offsets only, never
/// prompts, transcript content, account data, or credentials.
pub fn save_pending(ids: &[String]) -> Result<(), String> {
    if !ids.iter().all(|id| valid_id(id)) {
        return Err("Invalid thread ID".into());
    }
    let home = storage::codex_home();
    let targets = ids
        .iter()
        .map(|id| PendingTarget {
            id: id.clone(),
            offset: switcher::find_thread_rollout_path(&home, id)
                .and_then(|path| path.metadata().ok().map(|metadata| metadata.len())),
        })
        .collect::<Vec<_>>();
    write_manifest(&targets)
}

fn query(database: &Path, sql: &str) -> Result<String, String> {
    for attempt in 0..SQLITE_READ_ATTEMPTS {
        let output = Command::new("/usr/bin/sqlite3")
            .args([
                "-readonly",
                "-cmd",
                &format!(".timeout {SQLITE_BUSY_TIMEOUT_MS}"),
            ])
            .arg(database)
            .arg(sql)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        if attempt + 1 < SQLITE_READ_ATTEMPTS {
            sleep(Duration::from_millis(250 * (attempt as u64 + 1)));
        }
    }
    Err("Could not read Codex recovery state after bounded startup retries".into())
}

fn pending_count(home: &Path, id: &str) -> Result<usize, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(0);
    }
    query(
        &db,
        &format!("SELECT count(*) FROM queued_items WHERE thread_id = '{id}';"),
    )?
    .parse()
    .map_err(|_| "Invalid queue count".into())
}

fn queue_revision(home: &Path, id: &str) -> Result<u64, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(0);
    }
    let value = query(
        &db,
        &format!(
            "SELECT COALESCE((SELECT revision FROM queued_thread_revisions WHERE thread_id = '{id}'), 0);"
        ),
    )?;
    value.parse().map_err(|_| "Invalid queue revision".into())
}

fn validate_queue_snapshot_revision(before: u64, after: u64) -> Result<(), String> {
    if before == after {
        Ok(())
    } else {
        Err("Codex queue changed while recovery was reading it".into())
    }
}

fn parse_queued_rows(bytes: &[u8]) -> Result<Vec<Value>, String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    serde_json::from_slice(bytes).map_err(|_| "Codex queue returned invalid JSON".into())
}

fn queued_messages(home: &Path, id: &str) -> Result<Vec<Value>, String> {
    let db = home.join("queue_1.sqlite");
    if !db.exists() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT payload_json FROM queued_items WHERE thread_id = '{id}' ORDER BY queue_order;"
    );
    for attempt in 0..SQLITE_READ_ATTEMPTS {
        let mut child = Command::new("/usr/bin/sqlite3")
            .args([
                "-readonly",
                "-json",
                "-cmd",
                &format!(".timeout {SQLITE_BUSY_TIMEOUT_MS}"),
            ])
            .arg(&db)
            .arg(&sql)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or("Could not read Codex queue state")?
            .take(MAX_QUEUE_STATE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_QUEUE_STATE {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Codex queue state exceeds the recovery memory limit".into());
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if status.success() {
            let rows = parse_queued_rows(&bytes)?;
            let mut messages = Vec::with_capacity(rows.len());
            for row in rows {
                let payload = row["payload_json"]
                    .as_str()
                    .ok_or("Codex queue row is missing its payload")?;
                let message: Value = serde_json::from_str(payload)
                    .map_err(|_| "Codex queue contains an invalid message")?;
                if !message.is_object() {
                    return Err("Codex queue contains a non-object message".into());
                }
                messages.push(message);
            }
            return Ok(messages);
        }
        if attempt + 1 < SQLITE_READ_ATTEMPTS {
            sleep(Duration::from_millis(250 * (attempt as u64 + 1)));
        }
    }
    Err("Could not read Codex queued messages after bounded retries".into())
}

fn prepare_interrupted_queue(messages: &mut [Value]) -> Result<bool, String> {
    if messages.is_empty() {
        return Err("Codex queue changed while recovery was preparing it".into());
    }
    let mut changed = false;
    for message in messages {
        let object = message
            .as_object_mut()
            .ok_or("Codex queue contains a non-object message")?;
        match object.get("pausedReason") {
            Some(Value::String(reason)) if reason == INTERRUPTED_QUEUE_PAUSE => {
                object.remove("pausedReason");
                changed = true;
            }
            Some(Value::Null) | None => {}
            Some(_) => {
                return Err(
                    "Queued work has a non-restart pause reason; refusing to resume it".into(),
                )
            }
        }
    }
    Ok(changed)
}

#[derive(Default, Debug)]
struct Evidence {
    started: bool,
    work: bool,
    failed: bool,
    aborted: bool,
    start_time: Option<String>,
    work_time: Option<String>,
    start_turn_id: Option<String>,
}

impl Evidence {
    fn event(&mut self, line: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let payload = &value["payload"];
        let kind = payload["type"].as_str().unwrap_or("");
        let record = value["type"].as_str().unwrap_or("");
        let time = value["timestamp"].as_str().map(str::to_string);
        if record == "event_msg" && kind == "task_started" {
            self.started = true;
            self.failed = false;
            self.aborted = false;
            self.work = false;
            self.work_time = None;
            self.start_time = time;
            self.start_turn_id = payload["turn_id"].as_str().map(str::to_string);
        } else if record == "event_msg" && kind == "turn_aborted" {
            // The expected shutdown abort can appear after the pre-restart
            // checkpoint. A delayed terminal event for a different old turn
            // must not poison the IPC-confirmed replacement turn.
            let terminal_turn_id = payload["turn_id"].as_str();
            let matches_started = terminal_turn_id
                .map(|id| self.start_turn_id.as_deref() == Some(id))
                .unwrap_or(self.started);
            if self.started && matches_started {
                self.failed = true;
                self.aborted = true;
            } else if !self.started {
                self.aborted = true;
            }
        } else if record == "event_msg" && kind == "task_complete" && !payload["error"].is_null() {
            let terminal_turn_id = payload["turn_id"].as_str();
            let matches_started = terminal_turn_id
                .map(|id| self.start_turn_id.as_deref() == Some(id))
                .unwrap_or(self.started);
            if self.started && matches_started {
                self.failed = true;
            }
        } else if (record == "event_msg" && matches!(kind, "agent_message" | "agent_reasoning"))
            || (record == "response_item"
                && (matches!(
                    kind,
                    "agent_message"
                        | "reasoning"
                        | "function_call"
                        | "custom_tool_call"
                        | "web_search_call"
                ) || (kind == "message" && payload["role"].as_str() == Some("assistant"))))
        {
            self.work = true;
            self.work_time = time;
        }
    }

    fn matches_expected_turn(&self, expected_turn_id: Option<&str>) -> bool {
        self.started
            && expected_turn_id
                .map(|expected| self.start_turn_id.as_deref() == Some(expected))
                .unwrap_or(true)
    }

    fn verified(&self, expected_turn_id: Option<&str>) -> bool {
        // Work written after the initial checkpoint but before the old Desktop
        // finished shutting down is not recovery proof. A successful recovery
        // must contain both a fresh task_started boundary and substantive agent
        // work after that boundary.
        self.matches_expected_turn(expected_turn_id) && self.work && !self.failed && !self.aborted
    }
}

struct Observer {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    oversized: bool,
    evidence: Evidence,
}

impl Observer {
    fn checkpoint(path: PathBuf) -> Result<Self, String> {
        let offset = path.metadata().map_err(|e| e.to_string())?.len();
        Self::checkpoint_at(path, offset)
    }

    fn checkpoint_at(path: PathBuf, offset: u64) -> Result<Self, String> {
        let length = path.metadata().map_err(|e| e.to_string())?.len();
        if offset > length {
            return Err("Recovery checkpoint is past the end of its rollout".into());
        }
        Ok(Self {
            path,
            offset,
            partial: vec![],
            oversized: false,
            evidence: Evidence::default(),
        })
    }

    fn poll(&mut self) -> Result<(), String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        if length < self.offset {
            return Err("Rollout was truncated during recovery; cannot verify progress".into());
        }
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|e| e.to_string())?;
        // Snapshot length avoids chasing a live writer forever. Bounded chunks
        // preserve lifecycle records even when one tool result exceeds 128 KB.
        let mut remaining = length - self.offset;
        let mut buffer = [0_u8; 8192];
        while remaining > 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            let n = file.read(&mut buffer[..take]).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            self.offset += n as u64;
            remaining -= n as u64;
            for byte in &buffer[..n] {
                if *byte == b'\n' {
                    if !self.oversized {
                        self.evidence.event(&self.partial);
                    }
                    self.partial.clear();
                    self.oversized = false;
                } else if !self.oversized {
                    if self.partial.len() == MAX_LINE {
                        self.partial.clear();
                        self.oversized = true;
                    } else {
                        self.partial.push(*byte);
                    }
                }
            }
        }
        Ok(())
    }
}

fn should_dispatch(
    state: switcher::ThreadRolloutState,
    pending: usize,
    mode: RecoveryMode,
) -> bool {
    use switcher::ThreadRolloutState::*;
    // A writer lock proves only that Desktop has mounted/owns the thread. It
    // does not distinguish a running turn from an interrupted one. Therefore
    // ambiguous ActiveInProgress is dispatchable only when this operation owns
    // a pre-restart checkpoint or the user explicitly named the target.
    pending == 0
        && (matches!(state, InterruptedByQuota | TurnAborted)
            || (state == ActiveInProgress && mode.allows_ambiguous_active_dispatch()))
}

fn writer_is_locked(home: &Path, id: &str) -> bool {
    let path = home.join("thread-writer-locks").join(format!("{id}.lock"));
    match File::open(path) {
        Ok(file) => file.try_lock_exclusive().is_err(),
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

struct RecoveryTarget {
    id: String,
    state: switcher::ThreadRolloutState,
    writer_locked: bool,
    observer: Observer,
    existing_queue: usize,
    mounted_by_recovery: bool,
    dispatched: bool,
    completed: bool,
    failure: Option<String>,
    deadline: Instant,
    execution_deadline_set: bool,
    expected_turn_id: Option<String>,
    proof_observed_at: Option<Instant>,
}

fn prepare_target(
    home: &Path,
    id: &str,
    baseline: Option<u64>,
) -> Result<Option<RecoveryTarget>, String> {
    use switcher::ThreadRolloutState;
    if !valid_id(id) {
        return Err("Invalid thread ID".into());
    }
    let stored_id = query(&home.join("state_5.sqlite"), &format!(
        "SELECT id FROM threads WHERE id = '{id}' AND archived = 0 AND (thread_source IS NULL OR thread_source != 'subagent');"))?;
    if stored_id != id {
        return Err("Target is absent, archived, or a subagent; refusing recovery".into());
    }
    let state = switcher::inspect_thread_rollout_state(home, id);
    let pending = pending_count(home, id)?;
    if state == ThreadRolloutState::CleanCompleted && baseline.is_none() && pending == 0 {
        println!("RECOVERY_SKIPPED thread={id} reason=completed");
        return Ok(None);
    }
    if state == ThreadRolloutState::Unknown && baseline.is_none() {
        return Err("Unknown rollout state; refusing automatic recovery".into());
    }
    let path = switcher::find_thread_rollout_path(home, id).ok_or("Missing target rollout")?;
    let queued_once = pending > 0;
    let mut target = RecoveryTarget {
        id: id.to_string(),
        state,
        writer_locked: writer_is_locked(home, id),
        observer: match baseline {
            Some(offset) => Observer::checkpoint_at(path, offset)?,
            None => Observer::checkpoint(path)?,
        },
        existing_queue: pending,
        mounted_by_recovery: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now() + RECOVERY_DISPATCH_TIMEOUT,
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    };
    record_target_state(&mut target)?;
    // A target left in an older restart manifest may have been completed
    // manually since that failed run. If no new post-checkpoint work belongs to
    // this operation, completion is terminal and must not be revived or waited
    // on for three minutes.
    if target.state == ThreadRolloutState::CleanCompleted && !target.completed && !queued_once {
        println!("RECOVERY_SKIPPED thread={id} reason=completed");
        return Ok(None);
    }
    Ok(Some(target))
}

fn record_target_state_at(target: &mut RecoveryTarget, now: Instant) -> Result<(), String> {
    let was_started = target
        .observer
        .evidence
        .matches_expected_turn(target.expected_turn_id.as_deref());
    target.observer.poll()?;
    if target.dispatched
        && target.existing_queue > 0
        && target.expected_turn_id.is_none()
        && target.observer.evidence.started
    {
        target.expected_turn_id = target.observer.evidence.start_turn_id.clone();
        if let Some(turn_id) = target.expected_turn_id.as_deref() {
            println!(
                "RECOVERY_QUEUE_TURN_BOUND thread={} turn={} source=post_ack_rollout",
                target.id, turn_id
            );
        }
    }
    if let Some(expected) = target.expected_turn_id.as_deref() {
        if target.observer.evidence.started
            && target.observer.evidence.start_turn_id.as_deref() != Some(expected)
        {
            target.failure = Some(format!(
                "Desktop started unexpected turn {} instead of IPC-confirmed turn {expected}",
                target
                    .observer
                    .evidence
                    .start_turn_id
                    .as_deref()
                    .unwrap_or("unknown")
            ));
            return Ok(());
        }
    }
    let started = target
        .observer
        .evidence
        .matches_expected_turn(target.expected_turn_id.as_deref());
    if started && !was_started && !target.execution_deadline_set {
        // task_started proves that the app-server accepted dispatch, but long
        // threads can spend several minutes compacting before the first model
        // item. Keep waiting for substantive work without misreporting the
        // intermediate active state as successful recovery.
        target.deadline = Instant::now() + RECOVERY_EXECUTION_TIMEOUT;
        target.execution_deadline_set = true;
        println!(
            "RECOVERY_STARTED thread={} verification_timeout_secs={}",
            target.id,
            RECOVERY_EXECUTION_TIMEOUT.as_secs()
        );
    }
    if target.observer.evidence.failed {
        target.failure = Some(if target.observer.evidence.aborted {
            "Target was interrupted during recovery".into()
        } else {
            "Target reported an error during recovery".into()
        });
        return Ok(());
    }
    if target
        .observer
        .evidence
        .verified(target.expected_turn_id.as_deref())
    {
        let observed_at = *target.proof_observed_at.get_or_insert_with(|| {
            println!(
                "RECOVERY_WORK_OBSERVED thread={} start={} work={} soak_secs={}",
                target.id,
                target
                    .observer
                    .evidence
                    .start_time
                    .as_deref()
                    .unwrap_or("existing-turn"),
                target
                    .observer
                    .evidence
                    .work_time
                    .as_deref()
                    .unwrap_or("observed"),
                DESKTOP_STABILITY_WINDOW.as_secs()
            );
            now
        });
        if proof_survived_stability_window(observed_at, now) {
            if !target.completed {
                println!(
                    "RECOVERY_VERIFIED thread={} turn={} stable_secs={}",
                    target.id,
                    target
                        .expected_turn_id
                        .as_deref()
                        .unwrap_or("desktop-native"),
                    DESKTOP_STABILITY_WINDOW.as_secs()
                );
            }
            target.completed = true;
        }
    } else {
        target.proof_observed_at = None;
    }
    Ok(())
}

fn record_target_state(target: &mut RecoveryTarget) -> Result<(), String> {
    record_target_state_at(target, Instant::now())
}

fn proof_survived_stability_window(observed_at: Instant, now: Instant) -> bool {
    now.duration_since(observed_at) >= DESKTOP_STABILITY_WINDOW
}

fn dispatch_if_needed(
    home: &Path,
    desktop: &mut DesktopIpc,
    target: &mut RecoveryTarget,
    mode: RecoveryMode,
) -> Result<(), String> {
    if target.completed || target.failure.is_some() || target.observer.evidence.started {
        return Ok(());
    }
    let queue_revision_before = queue_revision(home, &target.id)?;
    let mut messages = queued_messages(home, &target.id)?;
    let queue_revision_after = queue_revision(home, &target.id)?;
    validate_queue_snapshot_revision(queue_revision_before, queue_revision_after)?;
    let pending = messages.len();
    if pending > 0 {
        target.existing_queue = pending;
        let unpaused = prepare_interrupted_queue(&mut messages)?;
        // Mark before IPC. A disconnect after forwarding has an unknown
        // outcome, so this operation must not retry the queue update.
        target.dispatched = true;
        target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
        if unpaused {
            target.mounted_by_recovery = desktop.resume_existing_queue(&target.id, messages)?;
            println!(
                "RECOVERY_QUEUE_UNPAUSED thread={} messages={} transport=desktop_ipc",
                target.id, pending
            );
        } else {
            // Mounting an already-unpaused queue wakes the owner's coordinator.
            let (_, mounted_by_recovery) = desktop.ensure_thread_owner(&target.id)?;
            target.mounted_by_recovery = mounted_by_recovery;
            println!(
                "RECOVERY_QUEUE_MOUNTED thread={} messages={} transport=desktop_ipc",
                target.id, pending
            );
        }
        return Ok(());
    }
    target.writer_locked = writer_is_locked(home, &target.id);
    if !should_dispatch(target.state, pending, mode) {
        return Err(format!(
            "Thread state {:?} is ambiguous without a pre-restart checkpoint; refusing to touch a possibly manually resumed task",
            target.state,
        ));
    }
    // Mark before the call. A timeout is an unknown outcome, so this operation
    // must never retry and risk starting the interrupted turn twice.
    target.dispatched = true;
    target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
    let (mounted_by_recovery, turn_id) = desktop.resume_interrupted_turn(&target.id)?;
    target.mounted_by_recovery = mounted_by_recovery;
    target.expected_turn_id = Some(turn_id.clone());
    println!(
        "RECOVERY_DISPATCHED thread={} turn={} transport=desktop_ipc trigger=app_update_resume",
        target.id, turn_id
    );
    Ok(())
}

pub fn recover_threads(ids: &[String], mode: RecoveryMode) -> Result<(), String> {
    let banner = RecoveryBanner::start(ids.len())?;
    recover_threads_with_banner(ids, mode, &banner)
}

pub(crate) fn recover_threads_with_banner(
    ids: &[String],
    mode: RecoveryMode,
    _banner: &RecoveryBanner,
) -> Result<(), String> {
    let home = storage::codex_home();
    let mut pending_manifest = load_manifest()?;
    for id in ids {
        if !valid_id(id) {
            return Err("Invalid thread ID".into());
        }
        // `save_pending` already captured pre-restart offsets. Recovery-only
        // calls reach this path without that earlier phase, so checkpoint them
        // here before any Desktop request is sent.
        let offset = switcher::find_thread_rollout_path(&home, id)
            .and_then(|path| path.metadata().ok().map(|metadata| metadata.len()));
        if let Some(target) = pending_manifest.iter_mut().find(|target| target.id == *id) {
            // A recovery-only request is a new operation and must never reuse a
            // stale offset left by an earlier failed restart. A captured restart
            // intentionally retains its pre-shutdown checkpoint.
            if !mode.captured() {
                target.offset = offset;
            }
        } else {
            pending_manifest.push(PendingTarget {
                id: id.clone(),
                offset,
            });
        }
    }

    write_manifest(&pending_manifest)?;
    let previous_pending = pending_manifest.clone();

    // Establish all checkpoints before actions in any target, so early work in
    // target N cannot be missed while target 1 is being dispatched.
    let mut targets = Vec::new();
    let mut completed_without_action = Vec::new();
    let mut preparation_failures = Vec::new();
    for id in ids {
        let previous = previous_pending.iter().find(|target| target.id == *id);
        match prepare_target(&home, id, previous.and_then(|target| target.offset)) {
            Ok(Some(target)) => targets.push(target),
            Ok(None) => completed_without_action.push(id.clone()),
            Err(error) => {
                eprintln!("RECOVERY_FAILED thread={id} reason={error}");
                preparation_failures.push(id.clone());
            }
        }
    }

    // Observe all targets for a short bounded grace before sending anything.
    // This catches work that Desktop or the user already resumed after the
    // checkpoint and prevents a duplicate empty turn without trusting locks.
    let activity_deadline = Instant::now() + PRE_DISPATCH_ACTIVITY_GRACE;
    while Instant::now() < activity_deadline {
        for target in &mut targets {
            if !target.completed && target.failure.is_none() {
                if let Err(error) = record_target_state(target) {
                    target.failure = Some(error);
                }
            }
        }
        if targets
            .iter()
            .all(|target| target.completed || target.failure.is_some())
        {
            break;
        }
        sleep(Duration::from_millis(200));
    }

    // Route each turn through its Desktop owner. Cold tasks are mounted only
    // after owner discovery says they are not already owned; no fixed UI sleep
    // is treated as a readiness contract.
    if !targets.is_empty() {
        match DesktopIpc::connect_with_retry(IPC_STARTUP_TIMEOUT) {
            Ok(mut desktop) => {
                println!("RECOVERY_CHANNEL_READY transport=desktop_ipc");
                for target in &mut targets {
                    if let Err(error) = dispatch_if_needed(&home, &mut desktop, target, mode) {
                        target.failure = Some(error);
                    }
                }
                drop(desktop);
            }
            Err(error) => {
                for target in &mut targets {
                    if record_target_state(target).is_err()
                        || (!target.completed && !target.observer.evidence.started)
                    {
                        target.failure = Some(error.clone());
                    }
                }
            }
        }
    }

    // A cold task is mounted only after authoritative owner discovery reports
    // no client. If that changed the visible task, restore the primary once;
    // never cycle through already-owned tasks or navigate again after proof.
    if let Some(primary) = ids.first() {
        let last_mounted = targets
            .iter()
            .rev()
            .find(|target| target.mounted_by_recovery)
            .map(|target| target.id.as_str());
        if last_mounted.is_some_and(|mounted| mounted != primary) {
            switcher::open_thread_in_codex(primary);
        }
    }

    loop {
        for target in &mut targets {
            if target.failure.is_some() || target.completed {
                continue;
            }
            if let Err(error) = record_target_state(target) {
                target.failure = Some(error);
            } else if (target.dispatched || target.execution_deadline_set)
                && Instant::now() >= target.deadline
            {
                target.failure = Some(if target.observer.evidence.started {
                    format!(
                        "Task started but produced no new agent work within {}s",
                        RECOVERY_EXECUTION_TIMEOUT.as_secs()
                    )
                } else if target.writer_locked {
                    format!(
                        "Desktop held the writer lock but produced no new agent work within {}s",
                        RECOVERY_EXECUTION_TIMEOUT.as_secs()
                    )
                } else {
                    format!(
                        "Desktop accepted recovery but no task started within {}s; dispatch was not retried",
                        RECOVERY_DISPATCH_TIMEOUT.as_secs()
                    )
                });
            }
        }
        if targets
            .iter()
            .all(|target| target.failure.is_some() || target.completed)
        {
            break;
        }
        sleep(Duration::from_millis(500));
    }

    let mut failures = preparation_failures;
    for target in &mut targets {
        if !target.completed && target.failure.is_none() {
            target.failure = Some("Recovery ended without verified agent work".into());
        }
        if let Some(error) = &target.failure {
            eprintln!("RECOVERY_FAILED thread={} reason={error}", target.id);
            failures.push(target.id.clone());
        } else {
            pending_manifest.retain(|item| item.id != target.id);
        }
    }
    for id in completed_without_action {
        pending_manifest.retain(|item| item.id != id);
    }
    write_manifest(&pending_manifest)?;

    failures.sort();
    failures.dedup();
    println!(
        "RECOVERY_RESULT verified_or_completed={} failed={}",
        ids.len().saturating_sub(failures.len()),
        failures.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Recovery unverified for {} thread(s): {}",
            failures.len(),
            failures.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use switcher::ThreadRolloutState::*;
    fn event(kind: &str, payload: Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"type":kind,"payload":payload,"timestamp":"test"}))
            .unwrap()
    }

    #[test]
    fn restart_operation_can_be_claimed_only_once() {
        let root = std::env::temp_dir().join(format!(
            "codex-restart-claim-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(claim_restart_operation_at(&root, "12345-678").unwrap());
        assert!(!claim_restart_operation_at(&root, "12345-678").unwrap());
        let metadata =
            std::fs::metadata(root.join("recovery-runs/restart-12345-678.claimed")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_operation_rejects_path_components() {
        assert!(!valid_operation_id("../worker"));
        assert!(!valid_operation_id("worker"));
        assert!(valid_operation_id("12345-678"));
    }

    #[test]
    fn durable_automation_cooldown_is_atomic_private_and_expires() {
        let root = std::env::temp_dir().join(format!(
            "codex-cooldown-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        arm_automation_cooldown_at(&root, now, Duration::from_secs(180)).unwrap();
        assert_eq!(
            automation_cooldown_remaining_at(&root, now).unwrap(),
            Some(Duration::from_secs(180))
        );
        assert_eq!(
            std::fs::metadata(cooldown_path(&root))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            automation_cooldown_remaining_at(&root, now + Duration::from_secs(180)).unwrap(),
            None
        );
        std::fs::write(cooldown_path(&root), b"invalid\n").unwrap();
        assert!(automation_cooldown_remaining_at(&root, now).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn desktop_ipc_dispatches_only_eligible_work() {
        for mode in [
            RecoveryMode::CapturedRestart,
            RecoveryMode::ExplicitTarget,
            RecoveryMode::DiscoveredOnly,
        ] {
            assert!(should_dispatch(TurnAborted, 0, mode));
            assert!(should_dispatch(InterruptedByQuota, 0, mode));
            assert!(!should_dispatch(InterruptedByQuota, 1, mode));
            assert!(!should_dispatch(CleanCompleted, 0, mode));
            assert!(!should_dispatch(Unknown, 0, mode));
        }
        assert!(should_dispatch(
            ActiveInProgress,
            0,
            RecoveryMode::CapturedRestart
        ));
        assert!(should_dispatch(
            ActiveInProgress,
            0,
            RecoveryMode::ExplicitTarget
        ));
        assert!(!should_dispatch(
            ActiveInProgress,
            0,
            RecoveryMode::DiscoveredOnly
        ));
    }

    #[test]
    fn ipc_framing_round_trips_and_rejects_invalid_lengths() {
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        let value = serde_json::json!({"type":"request","method":"initialize"});
        write_ipc_frame(&mut writer, &value).unwrap();
        assert_eq!(read_ipc_frame(&mut reader).unwrap(), value);

        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        writer.write_all(&0_u32.to_le_bytes()).unwrap();
        assert!(read_ipc_frame(&mut reader).is_err());

        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        writer
            .write_all(&((MAX_IPC_FRAME + 1) as u32).to_le_bytes())
            .unwrap();
        assert!(read_ipc_frame(&mut reader).is_err());
    }

    #[test]
    fn ipc_transport_outlives_the_router_timeout() {
        assert_eq!(
            ipc_response_wait("initialize", Duration::from_secs(5)),
            Duration::from_secs(7)
        );
        assert_eq!(
            ipc_response_wait("thread-owner-discovery", IPC_DISCOVERY_TIMEOUT),
            Duration::from_secs(24)
        );
        assert!(IPC_DISCOVERY_TIMEOUT > IPC_ROUTER_DISCOVERY_BUDGET);
    }

    #[test]
    fn recovery_turn_uses_one_protocol_valid_continue_text_input() {
        let thread_id = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
        let request = recovery_turn_start_request(thread_id);
        assert_eq!(request["conversationId"], thread_id);
        assert_eq!(request["turnStart"]["request"]["threadId"], thread_id);
        assert_eq!(
            request["turnStart"]["request"]["input"],
            serde_json::json!([{
                "type": "text",
                "text": "continue",
                "text_elements": []
            }])
        );
        assert_eq!(
            request["turnStart"]["request"]["turnTrigger"],
            "app_update_resume"
        );
        assert_eq!(
            request["turnStart"]["context"]["inheritThreadSettings"],
            true
        );
    }

    #[test]
    fn owner_and_start_responses_are_bound_to_the_expected_owner() {
        let owner_response = serde_json::json!({
            "method": "thread-owner-discovery",
            "handledByClientId": "owner-1",
            "result": { "supportsUntrustedAppInput": true }
        });
        assert_eq!(
            parse_owner_info(&owner_response).unwrap().client_id,
            "owner-1"
        );
        let mut rejected_owner = owner_response.clone();
        rejected_owner["result"]["supportsUntrustedAppInput"] = Value::Bool(false);
        assert!(parse_owner_info(&rejected_owner).is_err());

        let start_response = serde_json::json!({
            "method": "thread-follower-start-turn",
            "handledByClientId": "owner-1",
            "result": { "result": { "turn": { "id": "01a09c25-9480-7dc2-87fd-79c507f91fcb" } } }
        });
        assert_eq!(
            validate_start_response(&start_response, "owner-1").unwrap(),
            "01a09c25-9480-7dc2-87fd-79c507f91fcb"
        );
        assert!(validate_start_response(&start_response, "owner-2").is_err());
        let mut missing_turn = start_response.clone();
        missing_turn["result"]["result"]["turn"] = Value::Null;
        assert!(validate_start_response(&missing_turn, "owner-1").is_err());
        let mut malformed_turn = start_response;
        malformed_turn["result"]["result"]["turn"]["id"] = Value::String("turn\n1".into());
        assert!(validate_start_response(&malformed_turn, "owner-1").is_err());
    }

    #[test]
    fn only_the_ipc_confirmed_turn_can_satisfy_recovery() {
        let expected = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
        let other = "01a09c25-a442-78c0-9263-73ae757030e8";
        let mut evidence = Evidence::default();
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"task_started","turn_id":other}),
        ));
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(evidence.verified(Some(other)));
        assert!(!evidence.verified(Some(expected)));
    }

    #[test]
    fn proof_requires_the_full_stability_window() {
        let observed = Instant::now();
        assert!(!proof_survived_stability_window(
            observed,
            observed + DESKTOP_STABILITY_WINDOW - Duration::from_millis(1)
        ));
        assert!(proof_survived_stability_window(
            observed,
            observed + DESKTOP_STABILITY_WINDOW
        ));
    }

    #[test]
    fn delayed_terminal_for_an_old_turn_does_not_poison_the_replacement() {
        let expected = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
        let old = "01a09c25-a442-78c0-9263-73ae757030e8";
        let mut evidence = Evidence::default();
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"task_started","turn_id":expected}),
        ));
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"turn_aborted","turn_id":old}),
        ));
        assert!(evidence.verified(Some(expected)));
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"turn_aborted","turn_id":expected}),
        ));
        assert!(!evidence.verified(Some(expected)));
    }

    #[test]
    fn queue_recovery_strips_only_the_restart_pause() {
        let mut messages = vec![
            serde_json::json!({
                "id": "one",
                "pausedReason": INTERRUPTED_QUEUE_PAUSE,
                "context": { "keep": true }
            }),
            serde_json::json!({ "id": "two", "context": { "keep": true } }),
        ];
        assert!(prepare_interrupted_queue(&mut messages).unwrap());
        assert!(messages[0].get("pausedReason").is_none());
        assert_eq!(messages[0]["context"]["keep"], Value::Bool(true));
        assert_eq!(messages[1]["id"].as_str(), Some("two"));

        let mut manually_paused = vec![serde_json::json!({
            "id": "manual",
            "pausedReason": "Paused by the user"
        })];
        assert!(prepare_interrupted_queue(&mut manually_paused).is_err());
        assert_eq!(
            manually_paused[0]["pausedReason"].as_str(),
            Some("Paused by the user")
        );
    }
    #[test]
    fn queue_snapshot_requires_an_unchanged_revision() {
        assert!(validate_queue_snapshot_revision(41, 41).is_ok());
        assert!(validate_queue_snapshot_revision(41, 42).is_err());
    }
    #[test]
    fn empty_sqlite_json_output_is_an_empty_queue() {
        assert_eq!(parse_queued_rows(b"").unwrap(), Vec::<Value>::new());
        assert_eq!(parse_queued_rows(b"\n \t").unwrap(), Vec::<Value>::new());
        assert!(parse_queued_rows(b"not-json").is_err());
    }
    #[test]
    fn queue_start_metadata_and_tool_outputs_are_not_work() {
        let mut evidence = Evidence::default();
        for payload in [
            serde_json::json!({"type":"user_message","message":"continue"}),
            serde_json::json!({"type":"task_started"}),
            serde_json::json!({"type":"token_count"}),
        ] {
            evidence.event(&event("event_msg", payload));
        }
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"function_call_output"}),
        ));
        assert!(!evidence.verified(None));
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(evidence.verified(None));
        evidence.event(&event("event_msg", serde_json::json!({"type":"task_complete","error":{"codex_error_info":"usage_limit_exceeded"}})));
        assert!(!evidence.verified(None));
    }
    #[test]
    fn expected_restart_abort_before_new_turn_is_not_a_failure() {
        let mut evidence = Evidence::default();
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"turn_aborted"}),
        ));
        assert!(!evidence.failed);
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"task_started"}),
        ));
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(evidence.verified(None));
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"turn_aborted"}),
        ));
        assert!(evidence.failed);
    }
    #[test]
    fn work_followed_by_restart_abort_is_not_recovery_proof() {
        let mut evidence = Evidence::default();
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(!evidence.verified(None));
        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"turn_aborted"}),
        ));
        assert!(!evidence.verified(None));
        assert!(evidence.aborted);

        evidence.event(&event(
            "event_msg",
            serde_json::json!({"type":"task_started"}),
        ));
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(evidence.verified(None));
    }

    #[test]
    fn work_without_a_post_checkpoint_start_is_not_recovery_proof() {
        let mut evidence = Evidence::default();
        evidence.event(&event(
            "response_item",
            serde_json::json!({"type":"reasoning"}),
        ));
        assert!(evidence.work);
        assert!(!evidence.started);
        assert!(!evidence.verified(None));
    }
    #[test]
    fn legacy_pending_manifest_remains_readable() {
        let json = r#"["01a098c2-0fae-74d2-a80c-45d89e910e79"]"#;
        let stored: StoredManifest = serde_json::from_str(json).unwrap();
        let StoredManifest::Legacy(ids) = stored else {
            panic!("legacy manifest was not recognized")
        };
        assert_eq!(ids, ["01a098c2-0fae-74d2-a80c-45d89e910e79"]);
    }
    #[test]
    fn observer_ignores_old_work_and_handles_partial_and_huge_lines() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("cxi-observer-test-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        let work = event("response_item", serde_json::json!({"type":"reasoning"}));
        file.write_all(&work).unwrap();
        file.write_all(b"\n").unwrap();
        let mut observer = Observer::checkpoint(path.clone()).unwrap();
        observer.poll().unwrap();
        assert!(!observer.evidence.verified(None));
        let started = event("event_msg", serde_json::json!({"type":"task_started"}));
        file.write_all(&started).unwrap();
        file.write_all(b"\n").unwrap();
        file.write_all(&vec![b'x'; MAX_LINE + 10]).unwrap();
        file.write_all(b"\n").unwrap();
        file.write_all(&work[..20]).unwrap();
        observer.poll().unwrap();
        assert!(!observer.evidence.verified(None));
        file.write_all(&work[20..]).unwrap();
        file.write_all(b"\n").unwrap();
        observer.poll().unwrap();
        assert!(observer.evidence.verified(None));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_sql_or_url_injection() {
        assert!(valid_id("01a098c2-0fae-74d2-a80c-45d89e910e79"));
        assert!(!valid_id("' OR 1=1;--"));
        assert!(!valid_id(
            "codex://threads/01a098c2-0fae-74d2-a80c-45d89e910e79"
        ));
    }
}
