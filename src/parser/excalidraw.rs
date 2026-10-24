use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    /// The client decompresses with LZString.decompressFromBase64().
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<String>,
}

pub fn parse_excalidraw(content: &str) -> Option<ExcalidrawData> {
    // Pure .excalidraw JSON files
    if let Ok(data) = serde_json::from_str::<ExcalidrawData>(content) {
        if !data.elements.is_empty() || data.data_type.is_some() {
            return Some(data);
        }
    }

    // Obsidian markdown with excalidraw plugin – two embedded formats:
    // 1) Compressed base64 in frontmatter: `excalidraw-scene: <base64>`
    // 2) JSON in a code block at end of file
    extract_from_obsidian_md(content)
}

fn extract_from_obsidian_md(content: &str) -> Option<ExcalidrawData> {
    // 1) Obsidian Excalidraw plugin default: lz-string compressed JSON
    //    stored in a ```compressed-json ... ``` block under "## Drawing"
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
                        data_type: None,
                        version: None,
                        source: None,
                        elements: vec![],
                        app_state: None,
                        files: None,
                        compressed: Some(compressed),
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
                if let Ok(data) = serde_json::from_str::<ExcalidrawData>(candidate) {
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
            return serde_json::from_str::<ExcalidrawData>(candidate).ok();
        }
    }

    None
}
