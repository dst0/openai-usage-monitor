use super::ownerless_probe_rotation::OwnerlessProbeRotation;
use std::path::Path;

const HOME: &str = "/codex-rotation-test/home";

#[test]
fn rotation_visits_every_ownerless_target_in_order() {
    let rotation = OwnerlessProbeRotation::new(4);
    let picks: Vec<_> = (0..4)
        .map(|_| rotation.select(Path::new(HOME), 3))
        .collect();
    assert_eq!(picks, [Some(0), Some(1), Some(2), Some(0)]);
}

#[test]
fn pass_without_ownerless_targets_does_not_advance_rotation() {
    // One empty pass against three targets: an advance would yield Some(2).
    let rotation = OwnerlessProbeRotation::new(4);
    let home = Path::new(HOME);
    assert_eq!(rotation.select(home, 3), Some(0));
    assert_eq!(rotation.select(home, 0), None);
    assert_eq!(rotation.select(home, 3), Some(1));
}

#[test]
fn shrinking_ownerless_set_keeps_selection_in_range() {
    let rotation = OwnerlessProbeRotation::new(4);
    let home = Path::new(HOME);
    for _ in 0..5 {
        rotation.select(home, 4);
    }
    // Five passes over four targets leave the cursor at 1; once only one or
    // two targets remain, the next pick must still name one of them.
    assert_eq!(rotation.select(home, 2), Some(1));
    assert_eq!(rotation.select(home, 1), Some(0));
    assert_eq!(rotation.select(home, 3), Some(0));
}

#[test]
fn separate_rotations_do_not_share_a_cursor() {
    let first = OwnerlessProbeRotation::new(4);
    let second = OwnerlessProbeRotation::new(4);
    let home = Path::new(HOME);
    assert_eq!(first.select(home, 2), Some(0));
    assert_eq!(second.select(home, 2), Some(0));
    assert_eq!(first.select(home, 2), Some(1));
}

#[test]
fn each_home_keeps_its_own_turn() {
    // With one shared cursor, two passes over another home between each pass
    // over the primary would land the primary on the same index every time.
    let rotation = OwnerlessProbeRotation::new(4);
    let primary = Path::new("/codex-rotation-test/primary");
    let other = Path::new("/codex-rotation-test/other");
    let mut picks = Vec::new();
    for _ in 0..3 {
        picks.push(rotation.select(primary, 3));
        rotation.select(other, 1);
        rotation.select(other, 1);
    }
    assert_eq!(picks, [Some(0), Some(1), Some(2)]);
}

#[test]
fn full_rotation_forgets_the_least_recently_selected_home() {
    let rotation = OwnerlessProbeRotation::new(2);
    let first = Path::new("/codex-rotation-test/first");
    let second = Path::new("/codex-rotation-test/second");
    let third = Path::new("/codex-rotation-test/third");
    assert_eq!(rotation.select(first, 3), Some(0));
    assert_eq!(rotation.select(second, 3), Some(0));
    // Selecting `first` again makes `second` the least recently selected.
    assert_eq!(rotation.select(first, 3), Some(1));
    assert_eq!(rotation.select(third, 3), Some(0));
    // `first` kept its cursor; `second` was forgotten and starts over.
    assert_eq!(rotation.select(first, 3), Some(2));
    assert_eq!(rotation.select(second, 3), Some(0));
}

#[test]
fn single_home_rotation_keeps_only_the_latest_home() {
    let rotation = OwnerlessProbeRotation::new(1);
    let first = Path::new("/codex-rotation-test/first");
    let second = Path::new("/codex-rotation-test/second");
    assert_eq!(rotation.select(first, 2), Some(0));
    assert_eq!(rotation.select(second, 2), Some(0));
    assert_eq!(rotation.select(first, 2), Some(0));
}

#[test]
fn zero_capacity_rotation_is_rejected() {
    assert!(std::panic::catch_unwind(|| OwnerlessProbeRotation::new(0)).is_err());
}
