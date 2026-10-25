import type { App, TFile } from 'obsidian';
import type { ApiClient, ManifestEntry } from './api';

export interface SyncState {
	[path: string]: { checksum: string; size: number };
}

export interface SyncAction {
	type: 'push' | 'pull' | 'deleteRemote' | 'deleteLocal';
	path: string;
}

export interface SyncDiff {
	push: string[];
	pull: string[];
	deleteRemote: string[];
	deleteLocal: string[];
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

		// Files that exist locally
		for (const [path, file] of localMap) {
			const serverEntry = serverMap.get(path);
			const lastSync = this.lastSyncState[path];

			if (!serverEntry) {
				if (lastSync) {
					// Was synced before but gone from server → server deleted it
					deleteLocal.push(path);
				} else {
					// New local file → push
					push.push(path);
				}
			} else {
				// On server: compare checksums lazily using mtime
				const localMtime = file.stat.mtime;
				if (lastSync) {
					const serverChanged = serverEntry.checksum !== lastSync.checksum;
					const localMtimePast = lastSync.mtime !== undefined
						? localMtime <= (lastSync as any).mtime
						: false;
					if (serverChanged && localMtimePast) {
						pull.push(path);
					} else if (!serverChanged && localMtime > ((lastSync as any).mtime ?? 0) + 1000) {
						push.push(path);
					} else if (serverChanged && localMtime > ((lastSync as any).mtime ?? 0) + 1000) {
						// Both changed → last-write-wins: prefer local
						push.push(path);
					}
					// else: identical, skip
				} else {
					// Never synced: compare checksum by reading local
					const data = await this.app.vault.readBinary(file);
					const localChecksum = await sha256(data);
					if (localChecksum !== serverEntry.checksum) {
						push.push(path);
					}
				}
			}
		}

		// Files that exist on server but not locally
		for (const [path] of serverMap) {
			if (!localMap.has(path)) {
				const lastSync = this.lastSyncState[path];
				if (lastSync) {
					// Was here before, now gone locally → delete from server
					deleteRemote.push(path);
				} else {
					// New on server → pull
					pull.push(path);
				}
			}
		}

		return { push, pull, deleteRemote, deleteLocal };
	}

	async executeActions(
		diff: SyncDiff,
		onProgress: (msg: string, done: number, total: number) => void,
	): Promise<{ errors: string[] }> {
		const actions: SyncAction[] = [
			...diff.push.map(p => ({ type: 'push' as const, path: p })),
			...diff.pull.map(p => ({ type: 'pull' as const, path: p })),
			...diff.deleteRemote.map(p => ({ type: 'deleteRemote' as const, path: p })),
			...diff.deleteLocal.map(p => ({ type: 'deleteLocal' as const, path: p })),
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
					checksum,
					size: data.byteLength,
					mtime: file.stat.mtime,
				} as any;
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
				this.lastSyncState[action.path] = { checksum, size: data.byteLength } as any;
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
		onProgress: (msg: string, done: number, total: number) => void,
	): Promise<{ errors: string[]; diff: SyncDiff }> {
		const diff = await this.computeDiff();
		const { errors } = await this.executeActions(diff, onProgress);
		return { errors, diff };
	}

	private getLocalFiles(): TFile[] {
		return this.app.vault.getFiles();
	}
}
