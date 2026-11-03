use async_graphql::{Context, Object, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::AppState;
use crate::auth::CurrentUser;
use super::types::*;

pub struct QueryRoot;

fn get_user(ctx: &Context<'_>) -> Result<CurrentUser> {
    ctx.data::<CurrentUser>()
        .cloned()
        .map_err(|_| async_graphql::Error::new("Nicht authentifiziert"))
}

fn get_admin(ctx: &Context<'_>) -> Result<CurrentUser> {
    let user = get_user(ctx)?;
    if !user.is_admin {
        return Err(async_graphql::Error::new("Nur Admins"));
    }
    Ok(user)
}

#[Object(rename_fields = "snake_case", rename_args = "snake_case")]
impl QueryRoot {
    async fn me(&self, ctx: &Context<'_>) -> Option<UserWithPermissionsGql> {
        let user = ctx.data::<CurrentUser>().ok()?.clone();
        let state = ctx.data::<AppState>().ok()?;
        let users = crate::db::list_users_with_permissions(&state.db).await.ok()?;
        users.into_iter().find(|u| u.id == user.id).map(UserWithPermissionsGql::from)
    }

    async fn users(&self, ctx: &Context<'_>) -> Result<Vec<UserWithPermissionsGql>> {
        let user = get_user(ctx)?;
        if !user.is_admin && !user.can_manage_users {
            return Err(async_graphql::Error::new("Keine Berechtigung"));
        }
        let state = ctx.data::<AppState>()?;
        let users = crate::db::list_users_with_permissions(&state.db).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(users.into_iter().map(UserWithPermissionsGql::from).collect())
    }

    async fn groups(&self, ctx: &Context<'_>) -> Result<Vec<GroupGql>> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let groups = crate::db::list_groups_with_members(&state.db).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(groups.into_iter().map(GroupGql::from).collect())
    }

    async fn settings(&self, ctx: &Context<'_>) -> Result<SettingsGql> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        Ok(SettingsGql {
            publish_field: state.publish_field.read().unwrap().clone(),
            group_field: state.group_field.read().unwrap().clone(),
        })
    }

    async fn file_manifest(&self, ctx: &Context<'_>) -> Result<Vec<FileManifestEntryGql>> {
        get_user(ctx)?;
        let state = ctx.data::<AppState>()?;
        let vault_root = PathBuf::from(&state.config.server.vault_path);
        let mut entries = vec![];

        for ent in walkdir::WalkDir::new(&vault_root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = ent.path();
            let rel = match path.strip_prefix(&vault_root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if rel == crate::external_meta::META_FILENAME {
                continue;
            }
            let meta = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let Ok(bytes) = std::fs::read(path) else { continue };
            let checksum = hex::encode(Sha256::digest(&bytes));
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            entries.push(FileManifestEntryGql {
                path: rel,
                size: meta.len(),
                checksum,
                modified,
            });
        }
        Ok(entries)
    }

    async fn note_list(&self, ctx: &Context<'_>) -> Result<Vec<NoteEntryGql>> {
        get_user(ctx)?;
        let state = ctx.data::<AppState>()?;
        let vault = state.vault.read().await;
        let mut notes: Vec<NoteEntryGql> = vault.notes.values().map(|n| NoteEntryGql {
            slug: n.slug.clone(),
            title: n.title.clone(),
            kind: match n.kind {
                crate::vault::NoteKind::Markdown => "markdown",
                crate::vault::NoteKind::Canvas => "canvas",
                crate::vault::NoteKind::Excalidraw => "excalidraw",
                crate::vault::NoteKind::Base => "base",
            }.to_string(),
        }).collect();
        notes.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(notes)
    }

    async fn proposals(&self, ctx: &Context<'_>) -> Result<Vec<ProposalGql>> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let proposals = crate::db::list_proposals(&state.db).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(proposals.into_iter().map(ProposalGql::from).collect())
    }

    async fn all_note_meta(&self, ctx: &Context<'_>) -> Result<Vec<NoteMeta>> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let meta = crate::external_meta::read(&state.config.server.vault_path);
        Ok(meta.into_iter().map(|(slug, nm)| NoteMeta {
            slug,
            groups: nm.groups.unwrap_or_default(),
        }).collect())
    }
}
