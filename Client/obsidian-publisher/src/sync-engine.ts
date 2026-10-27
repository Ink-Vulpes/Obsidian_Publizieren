import type { App, TFile } from 'obsidian';
import type { ApiClient, ManifestEntry } from './api';

export interface SyncState {
	[path: string]: { checksum: string; size: number; mtime?: number };
}

export interface SyncDiff {
	push: string[];
	pull: string[];
	deleteRemote: string[];
	deleteLocal: string[];
	conflicts: string[];   // both local and server changed since last sync
}

export type ConflictResolution = 'push' | 'pull' | 'skip';

export interface SyncAction {
	type: 'push' | 'pull' | 'deleteRemote' | 'deleteLocal';
	path: string;
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

	async computeDiff(): Promise<SyncDiff> {
		const [serverManifest, localFiles] = await Promise.all([
			this.api.getManifest(),
			this.getLocalFiles(),
		]);

		const serverMap = new Map<string, ManifestEntry>(
			serverManifest.map(e => [e.path, e]),
		);
		const localMap = new Map<string, TFile>(
			localFiles.map(f => [f.path, f]),
		);

		const push: string[] = [];
		const pull: string[] = [];
		const deleteRemote: string[] = [];
		const deleteLocal: string[] = [];
		const conflicts: string[] = [];

		// Files that exist locally
		for (const [path, file] of localMap) {
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
				const localMtimeSinceSync = file.stat.mtime > ((lastSync?.mtime ?? 0) + 1000);

				if (!lastSync) {
					// Never synced: read local checksum and compare
					const data = await this.app.vault.readBinary(file);
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

		// Files that exist on server but not locally → always offer as download.
		// (If the user explicitly wants to delete them from the server, they can
		// deselect the file and use a future delete action — but auto-suggesting
		// "delete from server" when a file is simply missing locally is too risky.)
		for (const [path] of serverMap) {
			if (!localMap.has(path)) {
				pull.push(path);
			}
		}

		return { push, pull, deleteRemote, deleteLocal, conflicts };
	}

	async executeActions(
		diff: SyncDiff,
		conflictResolutions: Map<string, ConflictResolution>,
		onProgress: (msg: string, done: number, total: number) => void,
	): Promise<{ errors: string[] }> {
		const actions: SyncAction[] = [
			...diff.push.map(p => ({ type: 'push' as const, path: p })),
			...diff.pull.map(p => ({ type: 'pull' as const, path: p })),
			...diff.deleteRemote.map(p => ({ type: 'deleteRemote' as const, path: p })),
			...diff.deleteLocal.map(p => ({ type: 'deleteLocal' as const, path: p })),
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
				const file = this.app.vault.getFileByPath(action.path);
				if (!file) throw new Error('Datei nicht gefunden');
				const data = await this.app.vault.readBinary(file);
				const ok = await this.api.uploadFile(action.path, data);
				if (!ok) throw new Error('Upload fehlgeschlagen');
				const checksum = await sha256(data);
				this.lastSyncState[action.path] = {
					checksum, size: data.byteLength, mtime: file.stat.mtime,
				};
				break;
			}
			case 'pull': {
				const data = await this.api.downloadFile(action.path);
				const parts = action.path.split('/');
				if (parts.length > 1) {
					const dir = parts.slice(0, -1).join('/');
					if (!this.app.vault.getAbstractFileByPath(dir)) {
						await this.app.vault.createFolder(dir);
					}
				}
				const existing = this.app.vault.getFileByPath(action.path);
				if (existing) {
					await this.app.vault.modifyBinary(existing, data);
				} else {
					await this.app.vault.createBinary(action.path, data);
				}
				const checksum = await sha256(data);
				const updated = this.app.vault.getFileByPath(action.path);
				this.lastSyncState[action.path] = {
					checksum, size: data.byteLength, mtime: updated?.stat.mtime,
				};
				break;
			}
			case 'deleteRemote': {
				await this.api.deleteFile(action.path);
				delete this.lastSyncState[action.path];
				break;
			}
			case 'deleteLocal': {
				const file = this.app.vault.getFileByPath(action.path);
				if (file) await this.app.vault.delete(file);
				delete this.lastSyncState[action.path];
				break;
			}
		}
	}

	async syncAll(
		conflictResolutions: Map<string, ConflictResolution>,
		onProgress: (msg: string, done: number, total: number) => void,
	): Promise<{ errors: string[]; diff: SyncDiff }> {
		const diff = await this.computeDiff();
		const { errors } = await this.executeActions(diff, conflictResolutions, onProgress);
		return { errors, diff };
	}

	private getLocalFiles(): TFile[] {
		return this.app.vault.getFiles();
	}
}
