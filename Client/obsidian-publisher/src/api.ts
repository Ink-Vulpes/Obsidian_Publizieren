import type { PublisherSettings } from './settings';

export interface ManifestEntry {
	path: string;
	size: number;
	checksum: string;
	modified: number; // unix seconds
}

export interface GroupMember {
	id: number;
	username: string;
}

export interface Group {
	id: number;
	name: string;
	query: string | null;
	members: GroupMember[];
}

export interface UserWithPermissions {
	id: number;
	username: string;
	is_admin: boolean;
	created_at: string;
	permissions: {
		can_upload: boolean;
		can_manage_users: boolean;
		can_manage_lists: boolean;
		folder_restriction: boolean;
		sees_published: boolean;
		sees_all: boolean;
		can_propose: boolean;
	};
	allowed_folders: string[];
}

export class ApiClient {
	private settings: PublisherSettings;
	private token: string | null = null;

	constructor(settings: PublisherSettings) {
		this.settings = settings;
	}

	updateSettings(settings: PublisherSettings) {
		this.settings = settings;
		this.token = null;
	}

	private normalizeBase(): string {
		let base = this.settings.serverUrl.trim().replace(/\/$/, '');
		if (base && !base.match(/^https?:\/\//i)) {
			base = 'https://' + base;
		}
		return base;
	}

	private url(path: string): string {
		return `${this.normalizeBase()}${path}`;
	}

	private async graphql<T>(document: string, variables?: Record<string, unknown>): Promise<T> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/graphql'), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({ query: document, variables }),
		});
		if (!resp.ok) throw new Error(`GraphQL HTTP ${resp.status}`);
		const data = await resp.json();
		if (data.errors?.length) throw new Error(data.errors[0].message);
		return data.data as T;
	}

	async login(username: string, password: string): Promise<string> {
		const resp = await fetch(this.url('/graphql'), {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				query: 'mutation($u:String!,$p:String!){create_token(username:$u,password:$p)}',
				variables: { u: username, p: password },
			}),
		});
		if (!resp.ok) throw new Error(`Login fehlgeschlagen (HTTP ${resp.status})`);
		const data = await resp.json();
		if (data.errors?.length) throw new Error(data.errors[0].message || 'Login fehlgeschlagen');
		const token = data.data?.create_token;
		if (!token) throw new Error('Kein Token erhalten');
		this.token = token;
		return token;
	}

	async ensureToken(): Promise<string> {
		if (!this.token) {
			this.token = await this.login(this.settings.username, this.settings.password);
		}
		return this.token;
	}

	invalidateToken(): void {
		this.token = null;
	}

	// ── Vault ──────────────────────────────────────────────────────────────────

	async getManifest(): Promise<ManifestEntry[]> {
		const d = await this.graphql<{ file_manifest: ManifestEntry[] }>(
			'{file_manifest{path,size,checksum,modified}}'
		);
		return d.file_manifest;
	}

	async uploadFile(relPath: string, data: ArrayBuffer): Promise<boolean> {
		const token = await this.ensureToken();
		const boundary = '----ObsidianPluginUpload';
		const headerStr = [
			`--${boundary}`,
			`Content-Disposition: form-data; name="file"; filename="${relPath.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`,
			'Content-Type: application/octet-stream',
			`x-file-path: ${relPath}`,
			'',
			'',
		].join('\r\n');
		const footerStr = `\r\n--${boundary}--\r\n`;
		const headerBytes = new TextEncoder().encode(headerStr);
		const footerBytes = new TextEncoder().encode(footerStr);

		const body = new Uint8Array(headerBytes.length + data.byteLength + footerBytes.length);
		body.set(headerBytes, 0);
		body.set(new Uint8Array(data), headerBytes.length);
		body.set(footerBytes, headerBytes.length + data.byteLength);

		const resp = await fetch(this.url('/api/upload'), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': `multipart/form-data; boundary=${boundary}`,
			},
			body: body.buffer as ArrayBuffer,
		});
		if (!resp.ok) return false;
		const result = await resp.json();
		return (result.uploaded ?? 0) > 0;
	}

	async downloadFile(relPath: string): Promise<ArrayBuffer> {
		const token = await this.ensureToken();
		const resp = await fetch(
			this.url(`/api/sync/download?path=${encodeURIComponent(relPath)}`),
			{ headers: { 'Authorization': `Bearer ${token}` } },
		);
		if (!resp.ok) throw new Error(`Download fehlgeschlagen (HTTP ${resp.status}): ${relPath}`);
		return resp.arrayBuffer();
	}

	async deleteFile(relPath: string): Promise<boolean> {
		const d = await this.graphql<{ delete_file: boolean }>(
			'mutation($p:String!){delete_file(path:$p)}',
			{ p: relPath },
		);
		return d.delete_file;
	}

	async deleteAllFiles(): Promise<{ deleted: number; errors: string[] }> {
		const d = await this.graphql<{ delete_all_files: { deleted: number; errors: string[] } }>(
			'mutation{delete_all_files{deleted,errors}}'
		);
		return d.delete_all_files;
	}

	async reloadVault(): Promise<void> {
		await this.graphql<{ reload_vault: number }>('mutation{reload_vault}');
	}

	async registerDevice(deviceId: string, deviceName: string): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:String!,$n:String!){register_device(device_id:$id,device_name:$n)}',
			{ id: deviceId, n: deviceName },
		);
	}

	// ── Admin API ──────────────────────────────────────────────────────────────

	async adminListUsers(): Promise<UserWithPermissions[]> {
		const d = await this.graphql<{ users: UserWithPermissions[] }>(
			`{users{id,username,is_admin,created_at,allowed_folders,groups,
			  permissions{can_upload,can_manage_users,can_manage_lists,
			              folder_restriction,sees_published,sees_all,can_propose}}}`
		);
		return d.users;
	}

	async adminCreateInvite(): Promise<{ url: string; expires_hours: number }> {
		const d = await this.graphql<{ create_invite: { url: string; expires_hours: number } }>(
			'mutation{create_invite{url,expires_hours}}'
		);
		return d.create_invite;
	}

	async adminUpdatePermissions(
		userId: number,
		perms: {
			can_upload: boolean;
			can_manage_users: boolean;
			can_manage_lists: boolean;
			folder_restriction: boolean;
			sees_published: boolean;
			sees_all: boolean;
			can_propose: boolean;
			folders?: string[];
		},
	): Promise<void> {
		await this.graphql<unknown>(
			`mutation($uid:Int!,$i:UpdatePermissionsInput!){update_permissions(user_id:$uid,input:$i)}`,
			{
				uid: userId,
				i: {
					can_upload: perms.can_upload,
					can_manage_users: perms.can_manage_users,
					can_manage_lists: perms.can_manage_lists,
					folder_restriction: perms.folder_restriction,
					sees_published: perms.sees_published,
					sees_all: perms.sees_all,
					can_propose: perms.can_propose,
					folders: perms.folders ?? [],
				},
			},
		);
	}

	async adminDeleteUser(userId: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:Int!){delete_user(user_id:$id)}',
			{ id: userId },
		);
	}

	async adminGetSettings(): Promise<{ publish_field: string; group_field: string }> {
		const d = await this.graphql<{ settings: { publish_field: string; group_field: string } }>(
			'{settings{publish_field,group_field}}'
		);
		return d.settings;
	}

	async adminSetSettings(settings: { publish_field: string; group_field: string }): Promise<void> {
		await this.graphql<unknown>(
			'mutation($p:String!,$g:String){update_settings(publish_field:$p,group_field:$g)}',
			{ p: settings.publish_field, g: settings.group_field || null },
		);
	}

	// ── Group API ──────────────────────────────────────────────────────────────

	async adminListGroups(): Promise<Group[]> {
		const d = await this.graphql<{ groups: Group[] }>(
			'{groups{id,name,query,members{id,username}}}'
		);
		return d.groups;
	}

	async adminCreateGroup(name: string, query?: string): Promise<{ ok: boolean; id: number }> {
		const d = await this.graphql<{ create_group: number }>(
			'mutation($n:String!,$q:String){create_group(name:$n,query:$q)}',
			{ n: name, q: query || null },
		);
		return { ok: true, id: Number(d.create_group) };
	}

	async adminUpdateGroup(groupId: number, query: string | null): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:Int!,$q:String){update_group(id:$id,query:$q)}',
			{ id: groupId, q: query },
		);
	}

	async adminDeleteGroup(groupId: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:Int!){delete_group(id:$id)}',
			{ id: groupId },
		);
	}

	async adminAddGroupMember(groupId: number, userId: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($g:Int!,$u:Int!){add_group_member(group_id:$g,user_id:$u)}',
			{ g: groupId, u: userId },
		);
	}

	async adminRemoveGroupMember(groupId: number, userId: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($g:Int!,$u:Int!){remove_group_member(group_id:$g,user_id:$u)}',
			{ g: groupId, u: userId },
		);
	}

	// ── External note metadata API ─────────────────────────────────────────────

	async adminGetAllNoteMeta(): Promise<Record<string, { groups?: string[] }>> {
		const d = await this.graphql<{ all_note_meta: { slug: string; groups: string[] }[] }>(
			'{all_note_meta{slug,groups}}'
		);
		const result: Record<string, { groups?: string[] }> = {};
		for (const entry of d.all_note_meta) {
			result[entry.slug] = { groups: entry.groups.length ? entry.groups : undefined };
		}
		return result;
	}

	async adminSetNoteMeta(slug: string, groups: string[]): Promise<void> {
		await this.graphql<unknown>(
			'mutation($s:String!,$g:[String!]!){set_note_meta(slug:$s,groups:$g)}',
			{ s: slug, g: groups },
		);
	}

	// ── Proposals API ─────────────────────────────────────────────────────────

	async adminListProposals(): Promise<{
		id: number;
		note_slug: string;
		proposed_by: number;
		proposer_username: string;
		proposed_content: string;
		message: string | null;
		created_at: string;
	}[]> {
		const d = await this.graphql<{ proposals: unknown[] }>(
			'{proposals{id,note_slug,proposed_by,proposer_username,proposed_content,message,created_at}}'
		);
		return d.proposals as never;
	}

	async adminApproveProposal(id: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:Int!){approve_proposal(id:$id)}',
			{ id },
		);
	}

	async adminRejectProposal(id: number): Promise<void> {
		await this.graphql<unknown>(
			'mutation($id:Int!){reject_proposal(id:$id)}',
			{ id },
		);
	}

	async adminGetNoteList(): Promise<{ slug: string; title: string; kind: string }[]> {
		const d = await this.graphql<{ note_list: { slug: string; title: string; kind: string }[] }>(
			'{note_list{slug,title,kind}}'
		);
		return d.note_list;
	}
}
