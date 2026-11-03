use async_graphql::{Context, Object, Result};

use crate::AppState;
use crate::auth::CurrentUser;
use super::types::*;

pub struct MutationRoot;

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

fn get_upload_user(ctx: &Context<'_>) -> Result<CurrentUser> {
    let user = get_user(ctx)?;
    if !user.can_upload && !user.is_admin {
        return Err(async_graphql::Error::new("Keine Upload-Berechtigung"));
    }
    Ok(user)
}

#[Object(rename_fields = "snake_case", rename_args = "snake_case")]
impl MutationRoot {
    async fn create_token(
        &self,
        ctx: &Context<'_>,
        username: String,
        password: String,
    ) -> Result<String> {
        let state = ctx.data::<AppState>()?;
        let user = crate::db::find_user_by_username(&state.db, &username).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("Ungültige Zugangsdaten"))?;

        if !crate::auth::verify_password(&password, &user.password_hash) {
            return Err(async_graphql::Error::new("Ungültige Zugangsdaten"));
        }

        let perms = crate::db::get_user_permissions(&state.db, user.id).await.unwrap_or_default();
        let folders = crate::db::get_allowed_folders(&state.db, user.id).await.unwrap_or_default();
        let groups = crate::db::get_user_group_names(&state.db, user.id).await.unwrap_or_default();

        let token = crate::auth::create_jwt(
            user.id,
            &user.username,
            user.is_admin,
            perms.can_upload,
            perms.can_manage_users,
            perms.can_manage_lists,
            perms.folder_restriction,
            folders,
            groups,
            perms.sees_published,
            perms.sees_all,
            perms.can_propose,
            state.config.auth.session_duration_hours,
            &state.config.auth.session_secret,
        )
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        Ok(token)
    }

    async fn reload_vault(&self, ctx: &Context<'_>) -> Result<i32> {
        get_user(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::reload_vault(state).await;
        let count = state.vault.read().await.notes.len() as i32;
        Ok(count)
    }

    async fn delete_file(&self, ctx: &Context<'_>, path: String) -> Result<bool> {
        get_upload_user(ctx)?;
        let state = ctx.data::<AppState>()?;
        let vault_root = std::path::PathBuf::from(&state.config.server.vault_path);
        let target = vault_root.join(&path);
        let canonical_vault = vault_root.canonicalize().unwrap_or(vault_root.clone());
        let canonical_target = target.canonicalize()
            .map_err(|_| async_graphql::Error::new("Datei nicht gefunden"))?;
        if !canonical_target.starts_with(&canonical_vault) {
            return Err(async_graphql::Error::new("Pfad außerhalb des Vaults"));
        }
        std::fs::remove_file(&target)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let _ = crate::db::delete_vault_file(&state.db, &path).await;
        Ok(true)
    }

    async fn delete_all_files(&self, ctx: &Context<'_>) -> Result<DeleteAllResultGql> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let vault_root = std::path::PathBuf::from(&state.config.server.vault_path);
        let mut deleted = 0u32;
        let mut errors: Vec<String> = vec![];

        for ent in walkdir::WalkDir::new(&vault_root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Ok(rel) = ent.path().strip_prefix(&vault_root) {
                if rel.to_string_lossy().replace('\\', "/") == crate::external_meta::META_FILENAME {
                    continue;
                }
            }
            match std::fs::remove_file(ent.path()) {
                Ok(_) => {
                    if let Ok(rel) = ent.path().strip_prefix(&vault_root) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        let _ = crate::db::delete_vault_file(&state.db, &rel_str).await;
                    }
                    deleted += 1;
                }
                Err(e) => errors.push(format!("{}: {}", ent.path().display(), e)),
            }
        }
        Ok(DeleteAllResultGql { deleted, errors })
    }

    async fn register_device(
        &self,
        ctx: &Context<'_>,
        device_id: String,
        device_name: String,
    ) -> Result<bool> {
        let user = get_user(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::upsert_sync_device(&state.db, &device_id, &device_name, user.id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn create_invite(&self, ctx: &Context<'_>) -> Result<InvitePayloadGql> {
        let user = get_user(ctx)?;
        if !user.can_manage_users && !user.is_admin {
            return Err(async_graphql::Error::new("Keine Berechtigung"));
        }
        let state = ctx.data::<AppState>()?;
        let token = crate::auth::generate_token();
        let expires = chrono::Utc::now() + chrono::Duration::hours(48);
        crate::db::create_invite(&state.db, &token, user.id, expires).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let url = format!("{}/register/{}", state.config.server.base_url, token);
        Ok(InvitePayloadGql { url, token, expires_hours: 48 })
    }

    async fn update_permissions(
        &self,
        ctx: &Context<'_>,
        user_id: u32,
        input: UpdatePermissionsInput,
    ) -> Result<bool> {
        let caller = get_user(ctx)?;
        if !caller.can_manage_users && !caller.is_admin {
            return Err(async_graphql::Error::new("Keine Berechtigung"));
        }
        let state = ctx.data::<AppState>()?;
        if let Ok(Some(target)) = crate::db::find_user_by_id(&state.db, user_id).await {
            if target.is_admin {
                return Err(async_graphql::Error::new("Admin-Berechtigungen können nicht verändert werden"));
            }
        }
        let perms = crate::db::UserPermissions {
            user_id,
            can_upload: input.can_upload,
            can_manage_users: input.can_manage_users,
            can_manage_lists: input.can_manage_lists,
            folder_restriction: input.folder_restriction,
            sees_published: input.sees_published,
            sees_all: input.sees_all,
            can_propose: input.can_propose,
        };
        crate::db::upsert_permissions(&state.db, &perms).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let folders = input.folders.unwrap_or_default();
        let _ = crate::db::set_folder_access(&state.db, user_id, &folders).await;
        Ok(true)
    }

    async fn delete_user(&self, ctx: &Context<'_>, user_id: u32) -> Result<bool> {
        let caller = get_admin(ctx)?;
        if user_id == caller.id {
            return Err(async_graphql::Error::new("Kannst dich nicht selbst löschen"));
        }
        let state = ctx.data::<AppState>()?;
        if let Ok(Some(target)) = crate::db::find_user_by_id(&state.db, user_id).await {
            if target.is_admin {
                return Err(async_graphql::Error::new("Admin-Nutzer können nicht gelöscht werden"));
            }
        }
        crate::db::delete_user(&state.db, user_id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn create_group(
        &self,
        ctx: &Context<'_>,
        name: String,
        query: Option<String>,
    ) -> Result<i32> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(async_graphql::Error::new("Name darf nicht leer sein"));
        }
        let q = query.as_deref().and_then(|q| if q.trim().is_empty() { None } else { Some(q.trim()) });
        let id = crate::db::create_group(&state.db, &name, q).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        crate::reload_vault(state).await;
        Ok(id as i32)
    }

    async fn update_group(
        &self,
        ctx: &Context<'_>,
        id: u32,
        query: Option<String>,
    ) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let q = query.as_deref().and_then(|q| if q.trim().is_empty() { None } else { Some(q.trim()) });
        crate::db::update_group_query(&state.db, id, q).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        crate::reload_vault(state).await;
        Ok(true)
    }

    async fn delete_group(&self, ctx: &Context<'_>, id: u32) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::delete_group(&state.db, id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        crate::reload_vault(state).await;
        Ok(true)
    }

    async fn add_group_member(
        &self,
        ctx: &Context<'_>,
        group_id: u32,
        user_id: u32,
    ) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::add_user_to_group(&state.db, user_id, group_id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn remove_group_member(
        &self,
        ctx: &Context<'_>,
        group_id: u32,
        user_id: u32,
    ) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::remove_user_from_group(&state.db, user_id, group_id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn update_settings(
        &self,
        ctx: &Context<'_>,
        publish_field: String,
        group_field: Option<String>,
    ) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::set_setting(&state.db, "publish_field", &publish_field).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let gf = group_field.unwrap_or_else(|| "group".to_string());
        let gf = if gf.trim().is_empty() { "group".to_string() } else { gf.trim().to_string() };
        crate::db::set_setting(&state.db, "group_field", &gf).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        *state.publish_field.write().unwrap() = publish_field;
        *state.group_field.write().unwrap() = gf;
        crate::reload_vault(state).await;
        Ok(true)
    }

    async fn approve_proposal(&self, ctx: &Context<'_>, id: u32) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let proposal = crate::db::get_proposal(&state.db, id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new("Nicht gefunden"))?;

        let vault = state.vault.read().await;
        let entry = vault.notes.get(&proposal.note_slug)
            .ok_or_else(|| async_graphql::Error::new("Note nicht im Vault gefunden"))?.clone();
        drop(vault);

        let file_path = std::path::Path::new(&state.config.server.vault_path).join(&entry.path);
        std::fs::write(&file_path, &proposal.proposed_content)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        crate::reload_vault(state).await;
        let _ = crate::db::delete_proposal(&state.db, id).await;
        Ok(true)
    }

    async fn reject_proposal(&self, ctx: &Context<'_>, id: u32) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        crate::db::delete_proposal(&state.db, id).await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn set_note_meta(
        &self,
        ctx: &Context<'_>,
        slug: String,
        groups: Vec<String>,
    ) -> Result<bool> {
        get_admin(ctx)?;
        let state = ctx.data::<AppState>()?;
        let groups: Vec<String> = groups.into_iter()
            .map(|g| g.trim().to_lowercase())
            .filter(|g| !g.is_empty())
            .collect();
        let note_meta = crate::external_meta::NoteMeta {
            groups: if groups.is_empty() { None } else { Some(groups) },
        };
        crate::external_meta::update_note(&state.config.server.vault_path, &slug, note_meta)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        crate::reload_vault(state).await;
        Ok(true)
    }
}
