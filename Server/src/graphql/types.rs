use async_graphql::*;

#[derive(SimpleObject, Clone)]
#[graphql(rename_fields = "snake_case")]
pub struct UserPermissionsGql {
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub folder_restriction: bool,
    pub sees_published: bool,
    pub sees_all: bool,
    pub can_propose: bool,
    pub can_use_mcp: bool,
}

impl From<&crate::db::UserPermissions> for UserPermissionsGql {
    fn from(p: &crate::db::UserPermissions) -> Self {
        Self {
            can_upload: p.can_upload,
            can_manage_users: p.can_manage_users,
            can_manage_lists: p.can_manage_lists,
            folder_restriction: p.folder_restriction,
            sees_published: p.sees_published,
            sees_all: p.sees_all,
            can_propose: p.can_propose,
            can_use_mcp: p.can_use_mcp,
        }
    }
}

#[derive(SimpleObject, Clone)]
#[graphql(rename_fields = "snake_case")]
pub struct UserWithPermissionsGql {
    pub id: u32,
    pub username: String,
    pub is_admin: bool,
    pub created_at: String,
    pub permissions: UserPermissionsGql,
    pub allowed_folders: Vec<String>,
    pub groups: Vec<String>,
}

impl From<crate::db::UserWithPermissions> for UserWithPermissionsGql {
    fn from(u: crate::db::UserWithPermissions) -> Self {
        Self {
            id: u.id,
            username: u.username,
            is_admin: u.is_admin,
            created_at: u.created_at.to_rfc3339(),
            permissions: UserPermissionsGql::from(&u.permissions),
            allowed_folders: u.allowed_folders,
            groups: u.groups,
        }
    }
}

#[derive(SimpleObject, Clone)]
#[graphql(rename_fields = "snake_case")]
pub struct GroupMemberGql {
    pub id: u32,
    pub username: String,
}

#[derive(SimpleObject, Clone)]
#[graphql(rename_fields = "snake_case")]
pub struct GroupGql {
    pub id: u32,
    pub name: String,
    pub query: Option<String>,
    pub members: Vec<GroupMemberGql>,
}

impl From<crate::db::GroupWithMembers> for GroupGql {
    fn from(g: crate::db::GroupWithMembers) -> Self {
        Self {
            id: g.id,
            name: g.name,
            query: g.query,
            members: g.members.into_iter().map(|m| GroupMemberGql { id: m.id, username: m.username }).collect(),
        }
    }
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct SettingsGql {
    pub publish_field: String,
    pub group_field: String,
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct FileManifestEntryGql {
    pub path: String,
    pub size: u64,
    pub checksum: String,
    pub modified: i64,
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct NoteEntryGql {
    pub slug: String,
    pub title: String,
    pub kind: String,
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct ProposalGql {
    pub id: u32,
    pub note_slug: String,
    pub proposed_by: u32,
    pub proposer_username: String,
    pub proposed_content: String,
    pub message: Option<String>,
    pub created_at: String,
}

impl From<crate::db::Proposal> for ProposalGql {
    fn from(p: crate::db::Proposal) -> Self {
        Self {
            id: p.id,
            note_slug: p.note_slug,
            proposed_by: p.proposed_by,
            proposer_username: p.proposer_username,
            proposed_content: p.proposed_content,
            message: p.message,
            created_at: p.created_at.to_rfc3339(),
        }
    }
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct InvitePayloadGql {
    pub url: String,
    pub token: String,
    pub expires_hours: i32,
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct DeleteAllResultGql {
    pub deleted: u32,
    pub errors: Vec<String>,
}

#[derive(InputObject)]
#[graphql(rename_fields = "snake_case")]
pub struct UpdatePermissionsInput {
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub folder_restriction: bool,
    pub sees_published: bool,
    pub sees_all: bool,
    pub can_propose: bool,
    pub can_use_mcp: bool,
    pub folders: Option<Vec<String>>,
}

#[derive(SimpleObject)]
#[graphql(rename_fields = "snake_case")]
pub struct NoteMeta {
    pub slug: String,
    pub groups: Vec<String>,
}
