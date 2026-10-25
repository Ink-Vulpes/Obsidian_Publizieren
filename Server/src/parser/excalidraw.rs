use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExcalidrawData {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub elements: Vec<serde_json::Value>,
    #[serde(rename = "appState", skip_serializing_if = "Option::is_none")]
    pub app_state: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<serde_json::Value>,
    /// Raw lz-string base64 payload from Obsidian `compressed-json` blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<String>,
    /// fileId → vault filename from the `## Embedded Files` section.
    /// Not serialized — resolved into `files` before the struct is sent to the client.
    #[serde(skip)]
    pub embedded_file_refs: HashMap<String, String>,
}

pub fn parse_excalidraw(content: &str) -> Option<ExcalidrawData> {
    // Pure .excalidraw JSON files
    if let Ok(mut data) = serde_json::from_str::<ExcalidrawData>(content) {
        if !data.elements.is_empty() || data.data_type.is_some() {
            data.embedded_file_refs = HashMap::new();
            return Some(data);
        }
    }

    extract_from_obsidian_md(content)
}

fn extract_from_obsidian_md(content: &str) -> Option<ExcalidrawData> {
    let embedded_file_refs = parse_embedded_files_section(content);

    // 1) Obsidian Excalidraw plugin default: lz-string compressed JSON
    for marker in &["```compressed-json\n", "```compressed-json\r\n"] {
        if let Some(start) = content.find(marker) {
            let after = &content[start + marker.len()..];
            if let Some(end) = after.find("```") {
                let compressed: String = after[..end]
                    .chars()
                    .filter(|c| !c.is_ascii_whitespace())
                    .collect();
                if !compressed.is_empty() {
                    return Some(ExcalidrawData {
                        compressed: Some(compressed),
                        embedded_file_refs,
                        ..Default::default()
                    });
                }
            }
        }
    }

    // 2) Uncompressed JSON block
    let mut search = content;
    while let Some(start) = search.find("```json") {
        let after = &search[start + 7..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if candidate.contains("\"elements\"") {
                if let Ok(mut data) = serde_json::from_str::<ExcalidrawData>(candidate) {
                    data.embedded_file_refs = embedded_file_refs;
                    return Some(data);
                }
            }
            search = &after[end + 3..];
        } else {
            break;
        }
    }

    // 3) Fallback: ## Drawing section
    if let Some(start) = content.find("## Drawing\n```json") {
        let after = &content[start + 18..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            if let Ok(mut data) = serde_json::from_str::<ExcalidrawData>(candidate) {
                data.embedded_file_refs = embedded_file_refs;
                return Some(data);
            }
        }
    }

    None
}

/// Parse the `## Embedded Files` section:
///   <fileId>: [[vault-filename]]
/// Returns a map from fileId to vault filename.
fn parse_embedded_files_section(content: &str) -> HashMap<String, String> {
    let mut refs = HashMap::new();
    let header = "## Embedded Files";
    let Some(sec_start) = content.find(header) else {
        return refs;
    };
    let section = &content[sec_start + header.len()..];
    // Section ends at the next ## heading or end of content
    let section_end = section.find("\n##").unwrap_or(section.len());
    let section = &section[..section_end];

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        // Format: <fileId>: [[filename]] or <fileId>: filename
        let Some(colon) = line.find(": ") else { continue };
        let file_id = line[..colon].trim().to_string();
        let rest = line[colon + 2..].trim();
        let filename = if rest.starts_with("[[") && rest.ends_with("]]") {
            rest[2..rest.len() - 2].to_string()
        } else {
            rest.to_string()
        };
        if !file_id.is_empty() && !filename.is_empty() {
            refs.insert(file_id, filename);
        }
    }
    refs
}

/// Populate `data.files` by reading the vault assets referenced in `embedded_file_refs`.
/// Call this after `parse_excalidraw` whenever you have access to the vault filesystem.
pub fn resolve_embedded_files(
    data: &mut ExcalidrawData,
    vault_root: &std::path::Path,
    asset_paths: &HashMap<String, String>,
) {
    if data.embedded_file_refs.is_empty() {
        return;
    }

    use base64::Engine as _;

    let mut files = match &data.files {
        Some(serde_json::Value::Object(m)) => m.clone(),
        _ => serde_json::Map::new(),
    };

    for (file_id, vault_filename) in &data.embedded_file_refs {
        if files.contains_key(file_id) {
            continue;
        }

        let key = std::path::Path::new(vault_filename)
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let rel_path = match asset_paths.get(key.as_str()) {
            Some(p) => p,
            None => {
                tracing::warn!("Excalidraw embedded file not found in vault: {}", vault_filename);
                continue;
            }
        };

        let full_path = vault_root.join(rel_path);
        let bytes = match std::fs::read(&full_path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to read embedded file {:?}: {}", full_path, e);
                continue;
            }
        };

        let ext = std::path::Path::new(vault_filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "webp" => "image/webp",
            "avif" => "image/avif",
            _ => "application/octet-stream",
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);

        files.insert(
            file_id.clone(),
            serde_json::json!({
                "mimeType": mime,
                "id": file_id,
                "dataURL": data_url,
                "created": 0
            }),
        );
    }

    if !files.is_empty() {
        data.files = Some(serde_json::Value::Object(files));
    }
}
