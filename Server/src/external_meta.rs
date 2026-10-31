use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub const META_FILENAME: &str = "_server_metadata.json";

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct NoteMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
}

impl NoteMeta {
    pub fn is_empty(&self) -> bool {
        self.groups.as_ref().map(|g| g.is_empty()).unwrap_or(true)
    }
}

pub type ExternalMetaMap = HashMap<String, NoteMeta>;

pub fn meta_path(vault_root: &str) -> std::path::PathBuf {
    std::path::Path::new(vault_root).join(META_FILENAME)
}

pub fn read(vault_root: &str) -> ExternalMetaMap {
    let path = meta_path(vault_root);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write(vault_root: &str, meta: &ExternalMetaMap) -> anyhow::Result<()> {
    let path = meta_path(vault_root);
    let json = serde_json::to_string_pretty(meta)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn update_note(vault_root: &str, slug: &str, note_meta: NoteMeta) -> anyhow::Result<()> {
    let mut map = read(vault_root);
    if note_meta.is_empty() {
        map.remove(slug);
    } else {
        map.insert(slug.to_string(), note_meta);
    }
    write(vault_root, &map)
}
