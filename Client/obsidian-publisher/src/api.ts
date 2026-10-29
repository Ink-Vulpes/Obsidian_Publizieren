import type { PublisherSettings } from './settings';

export interface ManifestEntry {
	path: string;
	size: number;
	checksum: string;
	modified: number; // unix seconds
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

	async login(username: string, password: string): Promise<string> {
		const resp = await fetch(this.url('/api/auth/token'), {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ username, password }),
		});
		if (!resp.ok) throw new Error(`Login fehlgeschlagen (HTTP ${resp.status})`);
		const data = await resp.json();
		if (!data.token) throw new Error(data.error || 'Kein Token erhalten');
		this.token = data.token;
		return data.token;
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
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/files/manifest'), {
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error(`Manifest-Fehler: HTTP ${resp.status}`);
		return resp.json();
	}

	async uploadFile(relPath: string, data: ArrayBuffer): Promise<boolean> {
		const token = await this.ensureToken();
		const boundary = '----ObsidianPluginUpload';
		const headerStr = [
			`--${boundary}`,
			`Content-Disposition: form-data; name="file"; filename="${relPath}"`,
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
		const token = await this.ensureToken();
		const resp = await fetch(
			this.url(`/api/files/delete?path=${encodeURIComponent(relPath)}`),
			{ method: 'DELETE', headers: { 'Authorization': `Bearer ${token}` } },
		);
		return resp.ok;
	}

	async deleteAllFiles(): Promise<{ deleted: number; errors: string[] }> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/files/delete-all'), {
			method: 'DELETE',
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error(`Server-Reset fehlgeschlagen (HTTP ${resp.status})`);
		return resp.json();
	}

	async reloadVault(): Promise<void> {
		const token = await this.ensureToken();
		await fetch(this.url('/api/reload'), {
			headers: { 'Authorization': `Bearer ${token}` },
		});
	}

	async registerDevice(deviceId: string, deviceName: string): Promise<void> {
		const token = await this.ensureToken();
		await fetch(this.url('/api/sync/device'), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({ device_id: deviceId, device_name: deviceName }),
		});
	}

	// ── Admin JSON API ─────────────────────────────────────────────────────────

	async adminListUsers(): Promise<UserWithPermissions[]> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/admin/users'), {
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error('Keine Admin-Berechtigung oder Serverfehler');
		return resp.json();
	}

	async adminCreateInvite(): Promise<{ url: string; expires_hours: number }> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/admin/invite'), {
			method: 'POST',
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error('Einladung konnte nicht erstellt werden');
		return resp.json();
	}

	async adminUpdatePermissions(
		userId: number,
		perms: {
			can_upload: boolean;
			can_manage_users: boolean;
			can_manage_lists: boolean;
			folder_restriction: boolean;
			folders?: string[];
		},
	): Promise<void> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url(`/api/admin/users/${userId}/permissions`), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify(perms),
		});
		if (!resp.ok) throw new Error('Berechtigungen konnten nicht gespeichert werden');
	}

	async adminDeleteUser(userId: number): Promise<void> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url(`/admin/users/${userId}`), {
			method: 'DELETE',
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error('Benutzer konnte nicht gelöscht werden');
	}

	async adminGetLists(): Promise<{ whitelist: string[]; blacklist: string[] }> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/admin/lists'), {
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error('Listen nicht verfügbar');
		return resp.json();
	}

	async adminSetLists(whitelist: string[], blacklist: string[]): Promise<void> {
		const token = await this.ensureToken();
		await fetch(this.url('/api/admin/lists'), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({ whitelist, blacklist }),
		});
	}

	async adminGetSettings(): Promise<{ publish_field: string }> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/admin/settings'), {
			headers: { 'Authorization': `Bearer ${token}` },
		});
		if (!resp.ok) throw new Error('Einstellungen nicht verfügbar');
		return resp.json();
	}

	async adminSetSettings(settings: { publish_field: string }): Promise<void> {
		const token = await this.ensureToken();
		const resp = await fetch(this.url('/api/admin/settings'), {
			method: 'POST',
			headers: {
				'Authorization': `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify(settings),
		});
		if (!resp.ok) throw new Error('Einstellungen konnten nicht gespeichert werden');
	}
}
