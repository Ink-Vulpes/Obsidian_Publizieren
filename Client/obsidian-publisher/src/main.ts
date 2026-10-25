import { Notice, Plugin } from 'obsidian';
import { ApiClient } from './api';
import { SyncEngine } from './sync-engine';
import type { SyncState } from './sync-engine';
import { DEFAULT_SETTINGS, PublisherSettingTab } from './settings';
import type { PublisherSettings } from './settings';
import { SyncModal } from './modals/SyncModal';
import { AdminModal } from './modals/AdminModal';

interface PluginData {
	settings: PublisherSettings;
	syncState: SyncState;
	deviceId: string;
}

export default class ObsidianPublisher extends Plugin {
	settings: PublisherSettings;
	api: ApiClient;
	syncEngine: SyncEngine;

	private autoSyncTimer: number | null = null;
	private statusBar: HTMLElement;
	private deviceId: string;
	private syncState: SyncState = {};

	async onload() {
		await this.loadPluginData();

		this.api = new ApiClient(this.settings);
		this.syncEngine = new SyncEngine(this.app, this.api);
		this.syncEngine.loadSyncState(this.syncState);

		// Status bar
		this.statusBar = this.addStatusBarItem();
		this.setStatus('bereit');

		// Ribbon icon
		this.addRibbonIcon('cloud-upload', 'Publisher synchronisieren', () => this.openSyncModal());

		// Commands
		this.addCommand({
			id: 'sync-now',
			name: 'Jetzt synchronisieren',
			callback: () => this.openSyncModal(),
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

		// Settings tab
		this.addSettingTab(new PublisherSettingTab(this.app, this));

		// Auto-sync
		if (this.settings.autoSync) this.startAutoSync();

		// Sync on startup
		if (this.settings.syncOnStartup) {
			window.setTimeout(() => this.syncSilent(), 2000);
		}

		// Register device
		window.setTimeout(() => this.registerDevice(), 5000);
	}

	onunload() {
		this.stopAutoSync();
	}

	// ── Settings & data ────────────────────────────────────────────────────────

	private async loadPluginData() {
		const raw: Partial<PluginData> = await this.loadData() ?? {};
		this.settings = Object.assign({}, DEFAULT_SETTINGS, raw.settings ?? {});
		this.syncState = raw.syncState ?? {};
		this.deviceId = raw.deviceId ?? this.generateDeviceId();
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
		};
		await this.saveData(data);
	}

	// ── Auto-sync ──────────────────────────────────────────────────────────────

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
				await this.savePluginData();
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
			await this.savePluginData();
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
		} catch {
			// silent — not critical
		}
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
