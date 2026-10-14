use super::SessionMetaReader;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use std::fs::File;

fn thread_id_of(home: &TestRolloutHome, bytes: &[u8]) -> Option<String> {
    let path = home.path().join("probe.jsonl");
    std::fs::write(&path, bytes).unwrap();
    SessionMetaReader::thread_id(&File::open(path).unwrap()).unwrap()
}

#[test]
fn reads_the_id_of_a_leading_session_meta_record() {
    let home = TestRolloutHome::new("meta-ok");
    let thread = TestRolloutHome::thread_id(3);
    assert_eq!(
        thread_id_of(&home, &TestRolloutHome::rollout_bytes(&thread, 2)),
        Some(thread)
    );
}

#[test]
fn rejects_records_that_do_not_prove_the_thread() {
    let home = TestRolloutHome::new("meta-bad");
    for bytes in [
        &b""[..],
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"x\"}}",
        b"not json\n",
        b"{\"type\":\"response_item\",\"payload\":{\"id\":\"x\"}}\n",
        b"{\"type\":\"session_meta\",\"payload\":{}}\n",
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":7}}\n",
    ] {
        assert_eq!(thread_id_of(&home, bytes), None, "{:?}", String::from_utf8_lossy(bytes));
    }
}
