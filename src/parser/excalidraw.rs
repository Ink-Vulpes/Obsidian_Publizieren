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
    // Look for ```json ... ``` code block containing excalidraw scene
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

    // Fallback: any JSON block
    if let Some(start) = content.find("## Drawing\n```json") {
        let after = &content[start + 18..];
        if let Some(end) = after.find("```") {
            let candidate = after[..end].trim();
            return serde_json::from_str::<ExcalidrawData>(candidate).ok();
        }
    }

    None
}
