import { Notice, Plugin, WorkspaceLeaf } from 'obsidian';
import { ApiClient } from './api';
import { SyncEngine } from './sync-engine';
import type { SyncState } from './sync-engine';
import { DEFAULT_SETTINGS, PublisherSettingTab } from './settings';
import type { PublisherSettings } from './settings';
import { LogStore } from './log-store';
import type { LogEntry } from './log-store';
import { SyncLogView, SYNC_LOG_VIEW_TYPE } from './views/SyncLogView';
import { SyncModal } from './modals/SyncModal';
import { AdminModal } from './modals/AdminModal';
import { LogModal } from './modals/LogModal';

interface PluginData {
	settings: PublisherSettings;
	syncState: SyncState;
	deviceId: string;
	logEntries: LogEntry[];
}

export default class ObsidianPublisher extends Plugin {
	settings: PublisherSettings;
	api: ApiClient;
	syncEngine: SyncEngine;
	logStore: LogStore;

	private autoSyncTimer: number | null = null;
	private statusBar: HTMLElement;
	private deviceId: string;
	private syncState: SyncState = {};

	async onload() {
		await this.loadPluginData();

		this.api = new ApiClient(this.settings);
		this.syncEngine = new SyncEngine(this.app, this.api);
		this.syncEngine.loadSyncState(this.syncState);

		// ── Side-panel view ──────────────────────────────────────────────────────
		this.registerView(SYNC_LOG_VIEW_TYPE, leaf => new SyncLogView(leaf, this.logStore));

		// ── Status bar ───────────────────────────────────────────────────────────
		this.statusBar = this.addStatusBarItem();
		this.statusBar.style.cursor = 'pointer';
		this.statusBar.title = 'Publisher Log öffnen';
		this.statusBar.onclick = () => this.openLogPanel();
		this.setStatus('bereit');

		// ── Ribbon ───────────────────────────────────────────────────────────────
		this.addRibbonIcon('cloud-upload', 'Publisher synchronisieren', () => this.openSyncModal());
		this.addRibbonIcon('history', 'Publisher Log', () => this.openLogPanel());

		// ── Commands ─────────────────────────────────────────────────────────────
		this.addCommand({
			id: 'sync-now',
			name: 'Jetzt synchronisieren',
			callback: () => this.openSyncModal(),
		});

		this.addCommand({
			id: 'open-log-panel',
			name: 'Protokoll-Panel öffnen',
			callback: () => this.openLogPanel(),
		});

		this.addCommand({
			id: 'open-log-modal',
			name: 'Protokoll-Modal öffnen',
			callback: () => new LogModal(this.app, this.logStore).open(),
		});

		this.addCommand({
			id: 'open-admin',
			name: 'Admin-Panel öffnen',
			callback: () => new AdminModal(this.app, this.api).open(),
		});

		this.addCommand({
			id: 'sync-silent',
			name: 'Stille Synchronisierung (ohne Modal)',
			callback: () => this.syncSilent(),
		});

		// ── Settings tab ─────────────────────────────────────────────────────────
		this.addSettingTab(new PublisherSettingTab(this.app, this));

		// ── Auto-sync ────────────────────────────────────────────────────────────
		if (this.settings.autoSync) this.startAutoSync();

		if (this.settings.syncOnStartup) {
			window.setTimeout(() => this.syncSilent(), 2000);
		}

		window.setTimeout(() => this.registerDevice(), 5000);
	}

	onunload() {
		this.stopAutoSync();
		this.app.workspace.detachLeavesOfType(SYNC_LOG_VIEW_TYPE);
	}

	// ── Log panel ─────────────────────────────────────────────────────────────

	async openLogPanel() {
		const existing = this.app.workspace.getLeavesOfType(SYNC_LOG_VIEW_TYPE);
		if (existing.length > 0) {
			this.app.workspace.revealLeaf(existing[0]);
			return;
		}
		const leaf = this.app.workspace.getRightLeaf(false);
		if (leaf) {
			await leaf.setViewState({ type: SYNC_LOG_VIEW_TYPE, active: true });
			this.app.workspace.revealLeaf(leaf);
		}
	}

	// ── Settings & data ───────────────────────────────────────────────────────

	private async loadPluginData() {
		const raw: Partial<PluginData> = await this.loadData() ?? {};
		this.settings = Object.assign({}, DEFAULT_SETTINGS, raw.settings ?? {});
		this.syncState = raw.syncState ?? {};
		this.deviceId = raw.deviceId ?? this.generateDeviceId();
		this.logStore = new LogStore();
		this.logStore.load(raw.logEntries ?? []);
	}

	async saveSettings() {
		this.api?.updateSettings(this.settings);
		await this.savePluginData();
	}

	private async savePluginData() {
		const data: PluginData = {
			settings: this.settings,
			syncState: this.syncEngine?.getSyncState() ?? this.syncState,
			deviceId: this.deviceId,
			logEntries: this.logStore.serialize(),
		};
		await this.saveData(data);
	}

	// ── Auto-sync ─────────────────────────────────────────────────────────────

	startAutoSync() {
		this.stopAutoSync();
		const ms = this.settings.autoSyncIntervalMinutes * 60 * 1000;
		this.autoSyncTimer = window.setInterval(() => this.syncSilent(), ms);
	}

	stopAutoSync() {
		if (this.autoSyncTimer !== null) {
			window.clearInterval(this.autoSyncTimer);
			this.autoSyncTimer = null;
		}
	}

	// ── Sync ──────────────────────────────────────────────────────────────────

	private async openSyncModal() {
		if (!this.settings.serverUrl || !this.settings.username) {
			new Notice('Bitte zuerst Server-URL und Zugangsdaten in den Einstellungen konfigurieren.');
			return;
		}
		this.setStatus('berechne …');
		try {
			const diff = await this.syncEngine.computeDiff();
			new SyncModal(this.app, diff, this.syncEngine, async (errors) => {
				this.logStore.add({
					timestamp: Date.now(),
					push: diff.push,
					pull: diff.pull,
					deleteRemote: diff.deleteRemote,
					deleteLocal: diff.deleteLocal,
					errors,
				});
				await this.savePluginData();
				try { await this.api.reloadVault(); } catch { /* non-critical */ }
				if (errors.length > 0) {
					new Notice(`Sync abgeschlossen mit ${errors.length} Fehler(n):\n${errors[0]}`);
					this.setStatus(`${errors.length} Fehler`);
				} else {
					new Notice('Synchronisierung abgeschlossen.');
					this.setStatus('synchronisiert');
				}
			}).open();
			this.setStatus('bereit');
		} catch (e) {
			new Notice('Sync-Fehler: ' + (e as Error).message);
			this.setStatus('Fehler');
		}
	}

	private async syncSilent() {
		if (!this.settings.serverUrl || !this.settings.username) return;
		this.setStatus('synchronisiert …');
		try {
			const { errors, diff } = await this.syncEngine.syncAll((_msg, done, total) => {
				this.setStatus(`${done}/${total}`);
			});
			this.logStore.add({
				timestamp: Date.now(),
				push: diff.push,
				pull: diff.pull,
				deleteRemote: diff.deleteRemote,
				deleteLocal: diff.deleteLocal,
				errors,
			});
			await this.savePluginData();
			try { await this.api.reloadVault(); } catch { /* non-critical */ }
			const total = diff.push.length + diff.pull.length + diff.deleteRemote.length + diff.deleteLocal.length;
			if (errors.length > 0) {
				new Notice(`Auto-Sync: ${errors.length} Fehler`);
				this.setStatus(`${errors.length} Fehler`);
			} else if (total > 0) {
				new Notice(`Auto-Sync: ${total} Dateien synchronisiert`);
				this.setStatus('synchronisiert');
			} else {
				this.setStatus('synchronisiert');
			}
		} catch (e) {
			new Notice('Auto-Sync-Fehler: ' + (e as Error).message);
			this.setStatus('Fehler');
		}
	}

	// ── Device ────────────────────────────────────────────────────────────────

	private async registerDevice() {
		if (!this.settings.serverUrl || !this.settings.username) return;
		const name = this.settings.deviceName || `Obsidian (${navigator.platform})`;
		try {
			await this.api.registerDevice(this.deviceId, name);
		} catch { /* silent */ }
	}

	private generateDeviceId(): string {
		return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
			const r = Math.random() * 16 | 0;
			return (c === 'x' ? r : (r & 0x3 | 0x8)).toString(16);
		});
	}

	// ── Status bar ────────────────────────────────────────────────────────────

	private setStatus(msg: string) {
		this.statusBar.setText(`☁ ${msg}`);
	}
}
