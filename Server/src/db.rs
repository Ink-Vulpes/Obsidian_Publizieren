use chrono::{DateTime, Utc};
use sqlx::MySqlPool;

// ── Row types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: u32,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct UserPermissions {
    pub user_id: u32,
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub folder_restriction: bool,
    pub sees_published: bool,
    pub sees_all: bool,
}

impl Default for UserPermissions {
    fn default() -> Self {
        Self {
            user_id: 0,
            can_upload: false,
            can_manage_users: false,
            can_manage_lists: false,
            folder_restriction: false,
            sees_published: false,
            sees_all: false,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserWithPermissions {
    pub id: u32,
    pub username: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub permissions: UserPermissions,
    pub allowed_folders: Vec<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GroupMember {
    pub id: u32,
    pub username: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GroupWithMembers {
    pub id: u32,
    pub name: String,
    pub members: Vec<GroupMember>,
}

// ── Connection ───────────────────────────────────────────────────────────────

pub async fn create_pool(database_url: &str) -> anyhow::Result<MySqlPool> {
    let pool = sqlx::MySqlPool::connect(database_url).await?;
    Ok(pool)
}

/// Run all embedded schema migrations (idempotent – every statement uses IF NOT EXISTS).
pub async fn run_migrations(pool: &MySqlPool) -> anyhow::Result<()> {
    for sql in &[
        include_str!("../migrations/001_init.sql"),
        include_str!("../migrations/002_sync.sql"),
        include_str!("../migrations/003_settings.sql"),
        include_str!("../migrations/004_drop_sessions.sql"),
        include_str!("../migrations/005_groups.sql"),
        include_str!("../migrations/006_user_visibility.sql"),
    ] {
        for stmt in sql.split(";\n") {
            let s = stmt.trim();
            if s.is_empty() { continue; }
            match sqlx::query(s).execute(pool).await {
                Ok(_) => {}
                Err(sqlx::Error::Database(db_err))
                    // 42S21 = SQLState for duplicate column name (MySQL ER_DUP_FIELDNAME)
                    if db_err.code().as_deref() == Some("42S21") => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}

// ── Users ────────────────────────────────────────────────────────────────────

pub async fn count_users(pool: &MySqlPool) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn create_user(
    pool: &MySqlPool,
    username: &str,
    password_hash: &str,
    is_admin: bool,
) -> anyhow::Result<u32> {
    let res = sqlx::query("INSERT INTO users (username, password_hash, is_admin) VALUES (?, ?, ?)")
        .bind(username)
        .bind(password_hash)
        .bind(is_admin)
        .execute(pool)
        .await?;
    Ok(res.last_insert_id() as u32)
}

pub async fn find_user_by_username(pool: &MySqlPool, username: &str) -> anyhow::Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, is_admin, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?)
}

pub async fn find_user_by_id(pool: &MySqlPool, id: u32) -> anyhow::Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, is_admin, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_users(pool: &MySqlPool) -> anyhow::Result<Vec<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, is_admin, created_at FROM users ORDER BY id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_users_with_permissions(pool: &MySqlPool) -> anyhow::Result<Vec<UserWithPermissions>> {
    let users = list_users(pool).await?;
    let mut result = Vec::with_capacity(users.len());
    for u in users {
        let perms = get_user_permissions(pool, u.id).await?;
        let folders = get_allowed_folders(pool, u.id).await?;
        let groups = get_user_group_names(pool, u.id).await?;
        result.push(UserWithPermissions {
            id: u.id,
            username: u.username,
            is_admin: u.is_admin,
            created_at: u.created_at,
            permissions: perms,
            allowed_folders: folders,
            groups,
        });
    }
    Ok(result)
}

pub async fn delete_user(pool: &MySqlPool, user_id: u32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Permissions ──────────────────────────────────────────────────────────────

pub async fn get_user_permissions(pool: &MySqlPool, user_id: u32) -> anyhow::Result<UserPermissions> {
    let perms = sqlx::query_as::<_, UserPermissions>(
        "SELECT user_id, can_upload, can_manage_users, can_manage_lists, folder_restriction,
                COALESCE(sees_published, FALSE) AS sees_published,
                COALESCE(sees_all, FALSE) AS sees_all
         FROM user_permissions WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(perms.unwrap_or(UserPermissions { user_id, ..Default::default() }))
}

pub async fn upsert_permissions(pool: &MySqlPool, perms: &UserPermissions) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO user_permissions
           (user_id, can_upload, can_manage_users, can_manage_lists, folder_restriction, sees_published, sees_all)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
           can_upload = VALUES(can_upload),
           can_manage_users = VALUES(can_manage_users),
           can_manage_lists = VALUES(can_manage_lists),
           folder_restriction = VALUES(folder_restriction),
           sees_published = VALUES(sees_published),
           sees_all = VALUES(sees_all)",
    )
    .bind(perms.user_id)
    .bind(perms.can_upload)
    .bind(perms.can_manage_users)
    .bind(perms.can_manage_lists)
    .bind(perms.folder_restriction)
    .bind(perms.sees_published)
    .bind(perms.sees_all)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_allowed_folders(pool: &MySqlPool, user_id: u32) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT folder_path FROM folder_access WHERE user_id = ? ORDER BY folder_path")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(p,)| p).collect())
}

pub async fn set_folder_access(pool: &MySqlPool, user_id: u32, folders: &[String]) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM folder_access WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    for f in folders {
        sqlx::query("INSERT INTO folder_access (user_id, folder_path) VALUES (?, ?)")
            .bind(user_id)
            .bind(f)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// ── Invite tokens ─────────────────────────────────────────────────────────────

pub async fn create_invite(
    pool: &MySqlPool,
    token: &str,
    created_by: u32,
    expires_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO invite_tokens (token, created_by, expires_at) VALUES (?, ?, ?)")
        .bind(token)
        .bind(created_by)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn is_invite_valid(pool: &MySqlPool, token: &str) -> anyhow::Result<bool> {
    let now = Utc::now();
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT token FROM invite_tokens WHERE token = ? AND expires_at > ? AND used_at IS NULL",
    )
    .bind(token)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Mark invite as used and return true if it was valid.
pub async fn consume_invite(pool: &MySqlPool, token: &str) -> anyhow::Result<bool> {
    if !is_invite_valid(pool, token).await? {
        return Ok(false);
    }
    sqlx::query("UPDATE invite_tokens SET used_at = NOW() WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(true)
}

// ── Whitelist / Blacklist ────────────────────────────────────────────────────

pub async fn get_whitelist(pool: &MySqlPool) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT slug FROM note_whitelist ORDER BY slug")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

pub async fn get_blacklist(pool: &MySqlPool) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT slug FROM note_blacklist ORDER BY slug")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

pub async fn add_to_whitelist(pool: &MySqlPool, slug: &str, added_by: u32) -> anyhow::Result<()> {
    sqlx::query("INSERT IGNORE INTO note_whitelist (slug, added_by) VALUES (?, ?)")
        .bind(slug)
        .bind(added_by)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_from_whitelist(pool: &MySqlPool, slug: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM note_whitelist WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_to_blacklist(pool: &MySqlPool, slug: &str, added_by: u32) -> anyhow::Result<()> {
    sqlx::query("INSERT IGNORE INTO note_blacklist (slug, added_by) VALUES (?, ?)")
        .bind(slug)
        .bind(added_by)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_from_blacklist(pool: &MySqlPool, slug: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM note_blacklist WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_whitelist(pool: &MySqlPool, slugs: &[String], added_by: u32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM note_whitelist").execute(pool).await?;
    for slug in slugs {
        sqlx::query("INSERT IGNORE INTO note_whitelist (slug, added_by) VALUES (?, ?)")
            .bind(slug)
            .bind(added_by)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn set_blacklist(pool: &MySqlPool, slugs: &[String], added_by: u32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM note_blacklist").execute(pool).await?;
    for slug in slugs {
        sqlx::query("INSERT IGNORE INTO note_blacklist (slug, added_by) VALUES (?, ?)")
            .bind(slug)
            .bind(added_by)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// ── App settings ─────────────────────────────────────────────────────────────

pub async fn get_setting(pool: &MySqlPool, key: &str) -> anyhow::Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT `value` FROM app_settings WHERE `key` = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(v,)| v))
}

pub async fn set_setting(pool: &MySqlPool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (`key`, `value`) VALUES (?, ?)
         ON DUPLICATE KEY UPDATE `value` = VALUES(`value`)",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Vault file tracking ──────────────────────────────────────────────────────

pub async fn upsert_vault_file(
    pool: &MySqlPool,
    path: &str,
    checksum: &str,
    size: u64,
    uploaded_by: u32,
    device_id: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO vault_files (path, checksum, size, modified_at, uploaded_by, device_id)
         VALUES (?, ?, ?, NOW(), ?, ?)
         ON DUPLICATE KEY UPDATE
           checksum    = VALUES(checksum),
           size        = VALUES(size),
           modified_at = NOW(),
           uploaded_by = VALUES(uploaded_by),
           device_id   = VALUES(device_id)",
    )
    .bind(path)
    .bind(checksum)
    .bind(size as i64)
    .bind(uploaded_by)
    .bind(device_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_vault_file(pool: &MySqlPool, path: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM vault_files WHERE path = ?")
        .bind(path)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Sync devices ─────────────────────────────────────────────────────────────

// ── Groups ───────────────────────────────────────────────────────────────────

pub async fn create_group(pool: &MySqlPool, name: &str) -> anyhow::Result<u32> {
    let res = sqlx::query("INSERT INTO `groups` (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(res.last_insert_id() as u32)
}

pub async fn delete_group(pool: &MySqlPool, group_id: u32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM `groups` WHERE id = ?")
        .bind(group_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_groups(pool: &MySqlPool) -> anyhow::Result<Vec<Group>> {
    Ok(sqlx::query_as::<_, Group>(
        "SELECT id, name, created_at FROM `groups` ORDER BY name",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_groups_with_members(pool: &MySqlPool) -> anyhow::Result<Vec<GroupWithMembers>> {
    let groups = list_groups(pool).await?;
    let mut result = Vec::with_capacity(groups.len());
    for g in groups {
        let rows: Vec<(u32, String)> = sqlx::query_as(
            "SELECT u.id, u.username FROM users u
             JOIN user_groups ug ON ug.user_id = u.id
             WHERE ug.group_id = ?
             ORDER BY u.username",
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        result.push(GroupWithMembers {
            id: g.id,
            name: g.name,
            members: rows.into_iter().map(|(id, username)| GroupMember { id, username }).collect(),
        });
    }
    Ok(result)
}

pub async fn get_user_group_names(pool: &MySqlPool, user_id: u32) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT g.name FROM `groups` g
         JOIN user_groups ug ON ug.group_id = g.id
         WHERE ug.user_id = ?
         ORDER BY g.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

pub async fn add_user_to_group(pool: &MySqlPool, user_id: u32, group_id: u32) -> anyhow::Result<()> {
    sqlx::query("INSERT IGNORE INTO user_groups (user_id, group_id) VALUES (?, ?)")
        .bind(user_id)
        .bind(group_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_user_from_group(pool: &MySqlPool, user_id: u32, group_id: u32) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM user_groups WHERE user_id = ? AND group_id = ?")
        .bind(user_id)
        .bind(group_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Sync devices ─────────────────────────────────────────────────────────────

pub async fn upsert_sync_device(
    pool: &MySqlPool,
    device_id: &str,
    device_name: &str,
    user_id: u32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO sync_devices (device_id, device_name, user_id, last_seen)
         VALUES (?, ?, ?, NOW())
         ON DUPLICATE KEY UPDATE
           device_name = VALUES(device_name),
           user_id     = VALUES(user_id),
           last_seen   = NOW()",
    )
    .bind(device_id)
    .bind(device_name)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}
