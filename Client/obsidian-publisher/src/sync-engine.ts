import type { App, TFile } from 'obsidian';
import type { ApiClient, ManifestEntry } from './api';

export interface SyncState {
	[path: string]: { checksum: string; size: number; mtime?: number };
}

export interface SyncDiff {
	push: string[];
	pull: string[];
	serverOnly: string[];  // on server but not locally — user chooses download or delete
	deleteRemote: string[];
	deleteLocal: string[];
	conflicts: string[];   // both local and server changed since last sync
}

export type ConflictResolution = 'push' | 'pull' | 'skip';

export interface SyncAction {
	type: 'push' | 'pull' | 'deleteRemote' | 'deleteLocal';
	path: string;
}

// These .obsidian/ files are device-specific or ephemeral and excluded from sync.
const SETTINGS_EXCLUDE_LIST = new Set([
	'.obsidian/workspace.json',                              // open tabs / panel layout — per device
	'.obsidian/workspace-mobile.json',                       // mobile layout — per device
	'.obsidian/graph.json',                                  // graph view state (zoom/pan) — per device
	'.obsidian/plugins/obsidian-publisher/data.json',        // this plugin's own settings (credentials, device id, sync state)
]);

const SETTINGS_EXCLUDE_PREFIXES = [
	'.obsidian/cache',   // internal cache directory or cache.json
];

function isSettingsExcluded(path: string): boolean {
	if (SETTINGS_EXCLUDE_LIST.has(path)) return true;
	return SETTINGS_EXCLUDE_PREFIXES.some(ex => path === ex || path.startsWith(ex + '/') || path.startsWith(ex + '.'));
}

async function sha256(buffer: ArrayBuffer): Promise<string> {
	const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
	return Array.from(new Uint8Array(hashBuffer))
		.map(b => b.toString(16).padStart(2, '0'))
		.join('');
}

export class SyncEngine {
	private app: App;
	private api: ApiClient;
	private lastSyncState: SyncState = {};

	constructor(app: App, api: ApiClient) {
		this.app = app;
		this.api = api;
	}

	loadSyncState(state: SyncState) {
		this.lastSyncState = state;
	}

	getSyncState(): SyncState {
		return { ...this.lastSyncState };
	}

	clearSyncState() {
		this.lastSyncState = {};
	}

	// ── Local file helpers ────────────────────────────────────────────────────

	async readLocalFile(path: string): Promise<ArrayBuffer> {
		if (path.startsWith('.obsidian/')) {
			return this.app.vault.adapter.readBinary(path);
		}
		const file = this.app.vault.getFileByPath(path);
		if (!file) throw new Error('Datei nicht gefunden');
		return this.app.vault.readBinary(file);
	}

	async writeLocalFile(path: string, data: ArrayBuffer): Promise<void> {
		if (path.startsWith('.obsidian/')) {
			const parts = path.split('/');
			if (parts.length > 1) {
				const dir = parts.slice(0, -1).join('/');
				await this.ensureAdapterDir(dir);
			}
			await this.app.vault.adapter.writeBinary(path, data);
			return;
		}
		const parts = path.split('/');
		if (parts.length > 1) {
			const dir = parts.slice(0, -1).join('/');
			if (!this.app.vault.getAbstractFileByPath(dir)) {
				await this.app.vault.createFolder(dir);
			}
		}
		const existing = this.app.vault.getFileByPath(path);
		if (existing) {
			await this.app.vault.modifyBinary(existing, data);
		} else {
			await this.app.vault.createBinary(path, data);
		}
	}

	async deleteLocalFile(path: string): Promise<void> {
		if (path.startsWith('.obsidian/')) {
			try { await this.app.vault.adapter.remove(path); } catch { /* already gone */ }
			return;
		}
		const file = this.app.vault.getFileByPath(path);
		if (file) await this.app.vault.delete(file);
	}

	private async getFileMtime(path: string): Promise<number | undefined> {
		if (path.startsWith('.obsidian/')) {
			try {
				const stat = await this.app.vault.adapter.stat(path);
				return stat?.mtime;
			} catch { return undefined; }
		}
		return this.app.vault.getFileByPath(path)?.stat.mtime;
	}

	private async ensureAdapterDir(dir: string): Promise<void> {
		if (!dir || dir === '.') return;
		if (await this.app.vault.adapter.exists(dir)) return;
		const parent = dir.split('/').slice(0, -1).join('/');
		if (parent) await this.ensureAdapterDir(parent);
		await this.app.vault.adapter.mkdir(dir);
	}

	// ── File discovery ────────────────────────────────────────────────────────

	private getVaultFiles(): TFile[] {
		return this.app.vault.getFiles();
	}

	private async listSettingsFiles(): Promise<{ path: string; mtime: number }[]> {
		const results: { path: string; mtime: number }[] = [];
		await this.walkAdapterDir('.obsidian', results);
		return results.filter(f => !isSettingsExcluded(f.path));
	}

	private async walkAdapterDir(
		dir: string,
		results: { path: string; mtime: number }[],
	): Promise<void> {
		try {
			const listing = await this.app.vault.adapter.list(dir);
			for (const filePath of listing.files) {
				const stat = await this.app.vault.adapter.stat(filePath);
				results.push({ path: filePath, mtime: stat?.mtime ?? 0 });
			}
			for (const subDir of listing.folders) {
				await this.walkAdapterDir(subDir, results);
			}
		} catch { /* folder does not exist yet */ }
	}

	private async getAllLocalEntries(
		syncSettings: boolean,
	): Promise<{ path: string; mtime: number }[]> {
		const entries: { path: string; mtime: number }[] = this.getVaultFiles().map(f => ({
			path: f.path,
			mtime: f.stat.mtime,
		}));
		if (syncSettings) {
			const settingsEntries = await this.listSettingsFiles();
			entries.push(...settingsEntries);
		}
		return entries;
	}

	// ── Diff ──────────────────────────────────────────────────────────────────

	async computeDiff(syncSettings = false): Promise<SyncDiff> {
		const [serverManifest, localEntries] = await Promise.all([
			this.api.getManifest(),
			this.getAllLocalEntries(syncSettings),
		]);

		const serverMap = new Map<string, ManifestEntry>(
			serverManifest.map(e => [e.path, e]),
		);
		const localMap = new Map<string, number>(
			localEntries.map(e => [e.path, e.mtime]),
		);

		const push: string[] = [];
		const pull: string[] = [];
		const serverOnly: string[] = [];
		const deleteRemote: string[] = [];
		const deleteLocal: string[] = [];
		const conflicts: string[] = [];

		// Files that exist locally
		for (const [path, mtime] of localMap) {
			const serverEntry = serverMap.get(path);
			const lastSync = this.lastSyncState[path];

			if (!serverEntry) {
				if (lastSync) {
					// Was synced before but gone from server → server deleted it
					deleteLocal.push(path);
				} else {
					push.push(path);
				}
			} else {
				const serverChanged = serverEntry.checksum !== lastSync?.checksum;
				const localMtimeSinceSync = mtime > ((lastSync?.mtime ?? 0) + 1000);

				if (!lastSync) {
					// Never synced: compute local checksum and compare
					const data = await this.readLocalFile(path);
					const localChecksum = await sha256(data);
					if (localChecksum !== serverEntry.checksum) push.push(path);
				} else if (serverChanged && localMtimeSinceSync) {
					conflicts.push(path);
				} else if (serverChanged) {
					pull.push(path);
				} else if (localMtimeSinceSync) {
					push.push(path);
				}
				// else: identical, skip
			}
		}

		// Files that exist on server but not locally
		for (const [path] of serverMap) {
			if (!localMap.has(path)) {
				// Only surface settings files as serverOnly if syncSettings is on
				if (path.startsWith('.obsidian/') && !syncSettings) continue;
				serverOnly.push(path);
			}
		}

		return { push, pull, serverOnly, deleteRemote, deleteLocal, conflicts };
	}

	// ── Execute ───────────────────────────────────────────────────────────────

	async executeActions(
		diff: SyncDiff,
		conflictResolutions: Map<string, ConflictResolution>,
		onProgress: (msg: string, done: number, total: number) => void,
		serverOnlyActions: Map<string, 'pull' | 'deleteRemote'> = new Map(),
		deleteLocalActions: Map<string, 'deleteLocal' | 'push'> = new Map(),
	): Promise<{ errors: string[] }> {
		const actions: SyncAction[] = [
			...diff.push.map(p => ({ type: 'push' as const, path: p })),
			...diff.pull.map(p => ({ type: 'pull' as const, path: p })),
			...(diff.serverOnly ?? []).map(p => ({
				type: (serverOnlyActions.get(p) ?? 'pull') as 'pull' | 'deleteRemote',
				path: p,
			})),
			...diff.deleteRemote.map(p => ({ type: 'deleteRemote' as const, path: p })),
			...diff.deleteLocal.map(p => ({
				type: (deleteLocalActions.get(p) ?? 'deleteLocal') as 'deleteLocal' | 'push',
				path: p,
			})),
			// resolve conflicts
			...diff.conflicts.flatMap(p => {
				const res = conflictResolutions.get(p) ?? 'push';
				if (res === 'skip') return [];
				return [{ type: res as 'push' | 'pull', path: p }];
			}),
		];

		const errors: string[] = [];
		let done = 0;

		for (const action of actions) {
			onProgress(`${action.type}: ${action.path}`, done, actions.length);
			try {
				await this.executeOne(action);
			} catch (e) {
				errors.push(`${action.path}: ${(e as Error).message}`);
			}
			done++;
		}

		onProgress('Fertig', done, actions.length);
		return { errors };
	}

	private async executeOne(action: SyncAction): Promise<void> {
		switch (action.type) {
			case 'push': {
				const data = await this.readLocalFile(action.path);
				const ok = await this.api.uploadFile(action.path, data);
				if (!ok) throw new Error('Upload fehlgeschlagen');
				const checksum = await sha256(data);
				const mtime = await this.getFileMtime(action.path);
				this.lastSyncState[action.path] = { checksum, size: data.byteLength, mtime };
				break;
			}
			case 'pull': {
				const data = await this.api.downloadFile(action.path);
				await this.writeLocalFile(action.path, data);
				const checksum = await sha256(data);
				const mtime = await this.getFileMtime(action.path);
				this.lastSyncState[action.path] = { checksum, size: data.byteLength, mtime };
				break;
			}
			case 'deleteRemote': {
				await this.api.deleteFile(action.path);
				delete this.lastSyncState[action.path];
				break;
			}
			case 'deleteLocal': {
				await this.deleteLocalFile(action.path);
				delete this.lastSyncState[action.path];
				break;
			}
		}
	}

	async syncAll(
		conflictResolutions: Map<string, ConflictResolution>,
		onProgress: (msg: string, done: number, total: number) => void,
		syncSettings = false,
	): Promise<{ errors: string[]; diff: SyncDiff }> {
		const diff = await this.computeDiff(syncSettings);
		const { errors } = await this.executeActions(diff, conflictResolutions, onProgress);
		return { errors, diff };
	}

	// ── Vault-Initialisierung vom Server ──────────────────────────────────────

	async pullAllFromServer(
		onProgress: (msg: string, done: number, total: number) => void,
		includeSettings = false,
	): Promise<{ downloaded: number; errors: string[] }> {
		const manifest = await this.api.getManifest();

		const toDownload = includeSettings
			? manifest
			: manifest.filter(e => !e.path.startsWith('.obsidian/'));

		const errors: string[] = [];
		let downloaded = 0;

		for (let i = 0; i < toDownload.length; i++) {
			const entry = toDownload[i];
			onProgress(`Lade: ${entry.path}`, i, toDownload.length);
			try {
				const data = await this.api.downloadFile(entry.path);
				await this.writeLocalFile(entry.path, data);
				const checksum = await sha256(data);
				const mtime = await this.getFileMtime(entry.path);
				this.lastSyncState[entry.path] = { checksum, size: data.byteLength, mtime };
				downloaded++;
			} catch (e) {
				errors.push(`${entry.path}: ${(e as Error).message}`);
			}
		}

		onProgress('Fertig', toDownload.length, toDownload.length);
		return { downloaded, errors };
	}
}
