use super::pending_manifest::PendingManifest;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum StoredManifest {
    Current(PendingManifest),
    Legacy(Vec<String>),
}
