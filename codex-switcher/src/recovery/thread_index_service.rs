use super::{pending_target::PendingTarget, queue_snapshot::query, thread_identity::valid_id};
use std::{collections::HashMap, path::Path};

/// Read eligibility once for the entire journal. A separate SQLite retry for
/// every cold target can hold the recovery lock for many seconds per target.
pub(super) fn recent_thread_updates(
    home: &Path,
    targets: &[PendingTarget],
) -> Result<HashMap<String, i64>, String> {
    let ids = targets
        .iter()
        .filter(|target| valid_id(&target.id))
        .map(|target| format!("'{}'", target.id))
        .collect::<Vec<_>>()
        .join(",");
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = query(
        &home.join("state_5.sqlite"),
        &format!("SELECT id, updated_at FROM threads WHERE id IN ({ids}) AND archived = 0 AND (thread_source IS NULL OR thread_source != 'subagent');"),
    )?;
    let mut updates = HashMap::new();
    for row in rows.lines() {
        let (id, updated) = row
            .split_once('|')
            .ok_or("Invalid Codex thread index row")?;
        updates.insert(
            id.to_string(),
            updated
                .parse::<i64>()
                .map_err(|_| "Invalid Codex thread timestamp")?,
        );
    }
    Ok(updates)
}
