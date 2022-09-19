use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct IndexedLogMapConfig {
    pub storage_path: Option<PathBuf>,
    #[serde(default)]
    pub use_key_indexing: bool,
    #[serde(default)]
    pub readonly: bool,
}
