use axum::{
    extract::{Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub tags: Vec<String>,
}

pub async fn search(
    Query(params): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Json<Vec<SearchResult>> {
    let q = params.q.unwrap_or_default();
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Json(vec![]);
    }

    let vault = state.vault.read().await;
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<SearchResult> = vec![];

    // Title-priority matches first
    for note in vault.notes.values() {
        if note.title.to_lowercase().contains(&q) {
            seen.insert(note.slug.clone());
            results.push(SearchResult {
                title: note.title.clone(),
                slug: note.slug.clone(),
                excerpt: note.tags.join(", "),
                tags: note.tags.clone(),
            });
        }
    }

    // Full-text via pre-built search index
    for (slug, text) in &vault.search_index {
        if seen.contains(slug) {
            continue;
        }
        let lower = text.to_lowercase();
        if lower.contains(&q) {
            let pos = lower.find(&q).unwrap_or(0);
            let start = pos.saturating_sub(60);
            let end = (pos + q.len() + 60).min(text.len());
            // Clamp to char boundaries
            let start = text
                .char_indices()
                .map(|(i, _)| i)
                .filter(|&i| i <= start)
                .last()
                .unwrap_or(0);
            let end = text
                .char_indices()
                .map(|(i, _)| i)
                .find(|&i| i >= end)
                .unwrap_or(text.len());
            let excerpt = format!("…{}…", text[start..end].trim());

            if let Some(note) = vault.notes.get(slug) {
                seen.insert(slug.clone());
                results.push(SearchResult {
                    title: note.title.clone(),
                    slug: slug.clone(),
                    excerpt,
                    tags: note.tags.clone(),
                });
            }
        }
    }

    results.truncate(20);
    Json(results)
}

#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub link_count: usize,
}

#[derive(Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}

pub async fn graph_data(State(state): State<AppState>) -> Json<GraphData> {
    let vault = state.vault.read().await;

    let nodes: Vec<GraphNode> = vault
        .notes
        .values()
        .map(|n| {
            let backlink_count = vault
                .backlinks
                .get(&n.slug)
                .map(|v| v.len())
                .unwrap_or(0);
            GraphNode {
                id: n.slug.clone(),
                title: n.title.clone(),
                tags: n.tags.clone(),
                link_count: backlink_count + n.links_to.len(),
            }
        })
        .collect();

    let mut links = vec![];
    for note in vault.notes.values() {
        for link_title in &note.links_to {
            if let Some(target) = vault.title_to_slug.get(&link_title.to_lowercase()) {
                if vault.notes.contains_key(target) {
                    links.push(GraphLink {
                        source: note.slug.clone(),
                        target: target.clone(),
                    });
                }
            }
        }
    }

    Json(GraphData { nodes, links })
}

pub async fn file_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let vault = state.vault.read().await;
    let tree = crate::routes::note::build_file_tree_json(&vault);
    Json(tree)
}

pub async fn reload_vault(State(state): State<AppState>) -> Json<serde_json::Value> {
    match crate::vault::Vault::load(&state.config.vault_path) {
        Ok(new_vault) => {
            let count = new_vault.notes.len();
            let mut vault = state.vault.write().await;
            *vault = new_vault;
            Json(serde_json::json!({ "status": "ok", "notes": count }))
        }
        Err(e) => Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
    }
}
