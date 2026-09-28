use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const OLD_BIRTH: &str = "1726789012:000007";
const NEW_BIRTH: &str = "1726789999:000001";
const A: &str = "01a00000-0000-4000-8000-00000000000a";
const B: &str = "01a00000-0000-4000-8000-00000000000b";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "distribution-window-task-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { root }
    }

    fn helper(&self, ids: &[u32], fail_inventory: bool) -> SystemWindowRestoreBackend {
        self.helper_with_first_task(ids, fail_inventory, A)
    }

    fn helper_with_first_task(
        &self,
        ids: &[u32],
        fail_inventory: bool,
        first_task: &str,
    ) -> SystemWindowRestoreBackend {
        let inventory = json!({
            "process": {"pid": 4242, "birth_id": OLD_BIRTH},
            "window_ids": ids,
            "ambiguous_count": 0,
            "ax_standard_count": ids.len(),
        });
        let windows: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                json!({
                    "window_id": id,
                    "frame": {"x": (index as f64) * 950.0, "y": 30.0, "width": 900.0, "height": 700.0},
                    "task_id": if index == 0 { first_task } else { B },
                    "focused": index == 0,
                })
            })
            .collect();
        let snapshot = json!({
            "process": {"pid": 4242, "birth_id": OLD_BIRTH},
            "windows": windows,
            "clipboard_restored": true,
        });
        let restore = json!({
            "process": {"pid": 5151, "birth_id": NEW_BIRTH},
            "verified": vec![true; ids.len()],
            "clipboard_restored": true,
        });
        let inventory_answer = if fail_inventory {
            "printf 'WINDOW_ACCESS_FAILED\\n' >&2; exit 1".to_string()
        } else {
            format!("printf '%s' '{inventory}'")
        };
        let helper = self.root.join("helper");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n count-standard-windows) {inventory_answer} ;;\n snapshot-window-tasks) printf '%s' '{snapshot}' ;;\n restore-window-tasks) cat >/dev/null; printf 'restore\\n' >> '{calls}'; printf '%s' '{restore}' ;;\n *) exit 2 ;;\nesac\n",
            calls = self.root.join("calls").display(),
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        SystemWindowRestoreBackend::with_helper(helper)
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn restore_calls(&self) -> usize {
        std::fs::read_to_string(self.root.join("calls"))
            .map(|calls| calls.lines().count())
            .unwrap_or(0)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn old_process() -> ProcessIdentity {
    ProcessIdentity::new(4242, OLD_BIRTH).unwrap()
}

fn new_process() -> ProcessIdentity {
    ProcessIdentity::new(5151, NEW_BIRTH).unwrap()
}

#[test]
fn verified_zero_windows_keep_windowless_restart_path() {
    let fixture = Fixture::new("zero");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[], false),
            Err("unused home".into()),
        )
        .unwrap();
    assert_eq!(service.captured_window_ids().unwrap(), Some(Vec::new()));
    service.restore_with(
        Ok(new_process()),
        WindowTaskRestorePhase::AfterRelaunch,
        || panic!("zero windows must not open a restore helper"),
        || Ok(()),
    );
    assert_eq!(service.finish(), Ok(()));
}

#[test]
fn one_window_restores_its_selected_task_after_ready() {
    let fixture = Fixture::new("one");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[31], false),
            Ok(fixture.home()),
        )
        .unwrap();
    assert_eq!(service.captured_window_ids().unwrap(), Some(vec![31]));
    service.restore_with(
        Ok(new_process()),
        WindowTaskRestorePhase::AfterRelaunch,
        || Ok(fixture.helper(&[31], false)),
        || Ok(()),
    );
    assert_eq!(fixture.restore_calls(), 1);
    assert_eq!(service.finish(), Ok(()));
}

#[test]
fn two_windows_keep_exact_ids_and_recheck_after_recovery_target() {
    let fixture = Fixture::new("two");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[31, 32], false),
            Ok(fixture.home()),
        )
        .unwrap();
    assert_eq!(service.captured_window_ids().unwrap(), Some(vec![31, 32]));
    service.restore_with(
        Ok(new_process()),
        WindowTaskRestorePhase::AfterRelaunch,
        || Ok(fixture.helper(&[31, 32], false)),
        || Ok(()),
    );
    service.restore_with(
        Ok(new_process()),
        WindowTaskRestorePhase::AfterRecovery(&[A.into()]),
        || Ok(fixture.helper(&[31, 32], false)),
        || Ok(()),
    );
    assert_eq!(fixture.restore_calls(), 2);
    assert_eq!(service.finish(), Ok(()));
}

#[test]
fn access_failure_does_not_mean_zero_windows() {
    let fixture = Fixture::new("access");
    let service = DistributionWindowTaskService::default();
    let error = service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[], true),
            Ok(fixture.home()),
        )
        .unwrap_err();
    assert_eq!(error, "WINDOW_ACCESS_FAILED");
    assert!(service.captured_window_ids().is_err());
}

#[test]
fn window_appearing_after_zero_capture_refuses_final_pre_signal_check() {
    let fixture = Fixture::new("zero-to-one");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[], false),
            Err("unused".into()),
        )
        .unwrap();
    let error = service
        .verify_current_with(&old_process(), &mut fixture.helper(&[31], false))
        .unwrap_err();
    assert!(error.contains("appeared"), "{error}");
    assert_eq!(service.captured_window_ids().unwrap(), Some(Vec::new()));
}

#[test]
fn changed_selected_task_refuses_final_pre_signal_check() {
    let fixture = Fixture::new("task-changed");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[31], false),
            Ok(fixture.home()),
        )
        .unwrap();
    let mut signalled = false;
    let result = service
        .verify_current_with(
            &old_process(),
            &mut fixture.helper_with_first_task(&[31], false, B),
        )
        .map(|_| {
            signalled = true;
        });
    assert!(result.unwrap_err().contains("changed before shutdown"));
    assert!(!signalled);
    assert_eq!(service.captured_window_ids().unwrap(), Some(vec![31]));
}

#[test]
fn ipc_failure_records_missing_restore_without_sending_link() {
    let fixture = Fixture::new("ipc");
    let service = DistributionWindowTaskService::default();
    service
        .capture_with(
            &old_process(),
            &mut fixture.helper(&[31], false),
            Ok(fixture.home()),
        )
        .unwrap();
    service.restore_with(
        Ok(new_process()),
        WindowTaskRestorePhase::AfterRelaunch,
        || Ok(fixture.helper(&[31], false)),
        || Err("synthetic IPC wait failed".into()),
    );
    assert_eq!(fixture.restore_calls(), 0);
    assert!(service
        .finish()
        .unwrap_err()
        .contains("synthetic IPC wait failed"));
}

#[test]
fn changed_binding_after_ipc_wait_sends_no_task_link_in_either_pass() {
    for phase in [
        WindowTaskRestorePhase::AfterRelaunch,
        WindowTaskRestorePhase::AfterRecovery(&[A.into()]),
    ] {
        let fixture = Fixture::new("binding-changed");
        let service = DistributionWindowTaskService::default();
        service
            .capture_with(
                &old_process(),
                &mut fixture.helper(&[31], false),
                Ok(fixture.home()),
            )
            .unwrap();
        service.restore_with(
            Ok(new_process()),
            phase,
            || Ok(fixture.helper(&[31], false)),
            || Err("synthetic account or Desktop birth changed after IPC".into()),
        );
        assert_eq!(fixture.restore_calls(), 0);
        assert!(service.finish().is_err());
    }
}
