const ARCHIVE_PREFIXES: [&str; 3] = [
    "switcher",
    "account-switcher-daemon",
    "account-switcher-daemon-err",
];

pub(super) fn is_owned_archive(name: &str) -> bool {
    ARCHIVE_PREFIXES
        .iter()
        .any(|prefix| timestamped_archive(name, prefix))
}

pub(super) fn is_recovery_log(name: &str) -> bool {
    let Some(operation) = name
        .strip_prefix("restart-")
        .and_then(|rest| rest.strip_suffix(".log"))
    else {
        return false;
    };
    !operation.is_empty()
        && operation.len() <= 64
        && operation.contains('-')
        && operation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn is_redaction_temp(name: &str) -> bool {
    name.starts_with(".redact-") && name.ends_with(".tmp")
}

fn timestamped_archive(name: &str, prefix: &str) -> bool {
    let Some(timestamp) = name
        .strip_prefix(&format!("{prefix}-"))
        .and_then(|rest| rest.strip_suffix(".log.br"))
    else {
        return false;
    };
    timestamp.len() == 15
        && timestamp.as_bytes()[8] == b'-'
        && timestamp
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
}
