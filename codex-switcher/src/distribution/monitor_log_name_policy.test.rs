use super::monitor_log_name_policy::{is_recovery_log, is_redaction_temp};

#[test]
fn accepts_only_exact_numeric_redaction_temp_names() {
    assert!(is_redaction_temp(".redact-123-456.tmp"));
    for foreign in [
        ".redact-foreign.tmp",
        ".redact-1-.tmp",
        ".redact--2.tmp",
        ".redact-1-2-3.tmp",
        ".redact-1-x.tmp",
        "prefix.redact-1-2.tmp",
    ] {
        assert!(!is_redaction_temp(foreign), "accepted {foreign}");
    }
}

#[test]
fn accepts_only_exact_numeric_restart_log_names() {
    assert!(is_recovery_log("restart-1720000000000-123.log"));
    for foreign in [
        "restart-foreign.log",
        "restart-1-.log",
        "restart--2.log",
        "restart-1-2-3.log",
        "restart-1-x.log",
        "prefix-restart-1-2.log",
    ] {
        assert!(!is_recovery_log(foreign), "accepted {foreign}");
    }
}
