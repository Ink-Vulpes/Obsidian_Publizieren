import { Modal, Notice, Plugin, WorkspaceLeaf } from 'obsidian';
import { ApiClient } from './api';
import { SyncEngine } from './sync-engine';
import type { SyncState } from './sync-engine';
import { DEFAULT_SETTINGS, PublisherSettingTab } from './settings';
import type { PublisherSettings } from './settings';
import { LogStore } from './log-store';
import type { LogEntry } from './log-store';
import { SyncLogView, SYNC_LOG_VIEW_TYPE } from './views/SyncLogView';
import { ChangesView, CHANGES_VIEW_TYPE } from './views/ChangesView';
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

		// ── Views ────────────────────────────────────────────────────────────────
		this.registerView(SYNC_LOG_VIEW_TYPE, leaf => new SyncLogView(leaf, this.logStore));
		this.registerView(CHANGES_VIEW_TYPE, leaf => {
			const view = new ChangesView(leaf, this.syncEngine, this.api);
			view.setOnSyncDone(() => this.onSyncCompleted(null));
			return view;
		});

		// ── Status bar ───────────────────────────────────────────────────────────
		this.statusBar = this.addStatusBarItem();
		this.statusBar.style.cursor = 'pointer';
		this.statusBar.title = 'Publisher Log öffnen';
		this.statusBar.onclick = () => this.openLogPanel();
		this.setStatus('bereit');

		// ── Ribbon ───────────────────────────────────────────────────────────────
		this.addRibbonIcon('globe', 'Webseite öffnen', () => this.openSite());
		this.addRibbonIcon('git-compare', 'Ausstehende Änderungen', () => this.openChangesPanel());
		this.addRibbonIcon('history', 'Publisher Log', () => this.openLogPanel());
		this.addRibbonIcon('settings', 'Admin-Panel', () => new AdminModal(this.app, this.api).open());

		// ── Commands ─────────────────────────────────────────────────────────────
		this.addCommand({ id: 'open-changes', name: 'Ausstehende Änderungen anzeigen', callback: () => this.openChangesPanel() });
		this.addCommand({ id: 'open-log-panel', name: 'Protokoll-Panel öffnen', callback: () => this.openLogPanel() });
		this.addCommand({ id: 'open-log-modal', name: 'Protokoll-Modal öffnen', callback: () => new LogModal(this.app, this.logStore).open() });
		this.addCommand({ id: 'open-admin', name: 'Admin-Panel öffnen', callback: () => new AdminModal(this.app, this.api).open() });
		this.addCommand({ id: 'open-site', name: 'Webseite öffnen (eingeloggt)', callback: () => this.openSite() });
		this.addCommand({ id: 'sync-silent', name: 'Stille Synchronisierung (ohne Modal)', callback: () => this.syncSilent() });
		this.addCommand({ id: 'toggle-publish', name: 'Veröffentlichung umschalten', callback: () => this.togglePublishField() });
		this.addCommand({ id: 'reset-and-upload', name: 'Server zurücksetzen und Vault hochladen', callback: () => this.confirmResetAndUpload() });

		// ── Settings tab ─────────────────────────────────────────────────────────
		this.addSettingTab(new PublisherSettingTab(this.app, this));

		// ── Auto-sync ────────────────────────────────────────────────────────────
		if (this.settings.autoSync) this.startAutoSync();
		if (this.settings.syncOnStartup) window.setTimeout(() => this.syncSilent(), 2000);
		window.setTimeout(() => this.registerDevice(), 5000);
	}

	onunload() {
		this.stopAutoSync();
		this.app.workspace.detachLeavesOfType(SYNC_LOG_VIEW_TYPE);
		this.app.workspace.detachLeavesOfType(CHANGES_VIEW_TYPE);
	}

	// ── Panels ────────────────────────────────────────────────────────────────

	async openChangesPanel() {
		const existing = this.app.workspace.getLeavesOfType(CHANGES_VIEW_TYPE);
		if (existing.length > 0) {
			this.app.workspace.revealLeaf(existing[0]);
			return;
		}
		const leaf = this.app.workspace.getRightLeaf(false);
		if (leaf) {
			await leaf.setViewState({ type: CHANGES_VIEW_TYPE, active: true });
			this.app.workspace.revealLeaf(leaf);
		}
	}

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

	private refreshChangesPanel() {
		for (const leaf of this.app.workspace.getLeavesOfType(CHANGES_VIEW_TYPE)) {
			(leaf.view as ChangesView).refresh();
		}
	}

	// ── Sync lifecycle ────────────────────────────────────────────────────────

	private async onSyncCompleted(entry: LogEntry | null) {
		if (entry) this.logStore.add(entry);
		await this.savePluginData();
		try { await this.api.reloadVault(); } catch { /* non-critical */ }
		this.refreshChangesPanel();
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
		await this.saveData({
			settings: this.settings,
			syncState: this.syncEngine?.getSyncState() ?? this.syncState,
			deviceId: this.deviceId,
			logEntries: this.logStore.serialize(),
		} as PluginData);
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

	private async syncSilent() {
		if (!this.settings.serverUrl || !this.settings.username) return;
		this.setStatus('synchronisiert …');
		try {
			const { errors, diff } = await this.syncEngine.syncAll(
				new Map(),
				(_msg, done, total) => { this.setStatus(`${done}/${total}`); },
			);
			await this.onSyncCompleted({
				timestamp: Date.now(),
				push: diff.push,
				pull: [...diff.pull, ...(diff.serverOnly ?? [])],
				deleteRemote: diff.deleteRemote,
				deleteLocal: diff.deleteLocal,
				errors,
			});
			const total = diff.push.length + diff.pull.length +
				diff.deleteRemote.length + diff.deleteLocal.length;
			if (errors.length > 0) {
				new Notice(`Auto-Sync: ${errors.length} Fehler`);
				this.setStatus(`${errors.length} Fehler`);
			} else {
				this.setStatus(total > 0 ? 'synchronisiert' : 'bereit');
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
		try { await this.api.registerDevice(this.deviceId, name); } catch { /* silent */ }
	}

	private generateDeviceId(): string {
		return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
			const r = Math.random() * 16 | 0;
			return (c === 'x' ? r : (r & 0x3 | 0x8)).toString(16);
		});
	}

	private setStatus(msg: string) { this.statusBar.setText(`☁ ${msg}`); }

	// ── Open site ─────────────────────────────────────────────────────────────

	private async openSite(next = '/') {
		if (!this.settings.serverUrl) {
			new Notice('Bitte zuerst die Server-URL in den Einstellungen konfigurieren.');
			return;
		}
		let url: string;
		try {
			const token = await this.api.ensureToken();
			const base = this.settings.serverUrl.replace(/\/$/, '');
			url = `${base}/auto-login?token=${encodeURIComponent(token)}&next=${encodeURIComponent(next)}`;
		} catch (e) {
			new Notice('Anmeldung fehlgeschlagen: ' + (e as Error).message);
			return;
		}

		// Try Surfing (internal browser plugin) first
		const surfing = (this.app as any).plugins?.plugins?.['surfing'];
		if (surfing) {
			const leaf = this.app.workspace.getLeaf('tab');
			await leaf.setViewState({ type: 'surfing-view', state: { url } });
			this.app.workspace.revealLeaf(leaf);
			return;
		}

		// Fallback: system default browser
		window.open(url, '_blank');
	}

	// ── Reset & upload ────────────────────────────────────────────────────────

	private confirmResetAndUpload() {
		if (!this.settings.serverUrl || !this.settings.username) {
			new Notice('Bitte zuerst die Server-URL in den Einstellungen konfigurieren.');
			return;
		}
		new ResetConfirmModal(this.app, () => this.resetAndUpload()).open();
	}

	private async resetAndUpload() {
		this.setStatus('löscht Server …');
		try {
			const { deleted } = await this.api.deleteAllFiles();
			new Notice(`✓ ${deleted} Dateien vom Server gelöscht.`);
		} catch (e) {
			new Notice('Server-Reset fehlgeschlagen: ' + (e as Error).message);
			this.setStatus('Fehler');
			return;
		}

		// Clear local sync state so all local files appear new
		this.syncEngine.clearSyncState();
		await this.savePluginData();

		// Reload server vault index
		try { await this.api.reloadVault(); } catch { /* non-critical */ }

		// Upload everything
		this.setStatus('lädt hoch …');
		try {
			const files = this.app.vault.getFiles();
			let done = 0;
			const errors: string[] = [];
			for (const file of files) {
				this.setStatus(`${done}/${files.length}`);
				try {
					const data = await this.app.vault.readBinary(file);
					await this.api.uploadFile(file.path, data);
				} catch (e) {
					errors.push(`${file.path}: ${(e as Error).message}`);
				}
				done++;
			}
			await this.onSyncCompleted(null);
			if (errors.length > 0) {
				new Notice(`Upload abgeschlossen. ${errors.length} Fehler.`);
				this.setStatus(`${errors.length} Fehler`);
			} else {
				new Notice(`✓ ${files.length} Dateien hochgeladen.`);
				this.setStatus('bereit');
			}
		} catch (e) {
			new Notice('Upload fehlgeschlagen: ' + (e as Error).message);
			this.setStatus('Fehler');
		}
	}

	// ── Publish toggle ────────────────────────────────────────────────────────

	private async togglePublishField() {
		const file = this.app.workspace.getActiveFile();
		if (!file) {
			new Notice('Keine Datei geöffnet.');
			return;
		}

		// Get configured publish field name from server
		let fieldName = 'published';
		try {
			const settings = await this.api.adminGetSettings();
			fieldName = settings.publish_field || 'published';
		} catch { /* use default */ }

		const content = await this.app.vault.read(file);
		const newContent = toggleFrontmatterBool(content, fieldName);
		await this.app.vault.modify(file, newContent);

		const isNowPublished = getFrontmatterBool(newContent, fieldName);
		new Notice(isNowPublished ? `✓ Veröffentlicht (${fieldName}: true)` : `○ Versteckt (${fieldName}: false)`);
	}
}

class ResetConfirmModal extends Modal {
	private onConfirm: () => void;

	constructor(app: import('obsidian').App, onConfirm: () => void) {
		super(app);
		this.onConfirm = onConfirm;
	}

	onOpen() {
		const { contentEl } = this;
		contentEl.createEl('h2', { text: 'Server zurücksetzen' });
		contentEl.createEl('p', {
			text: '⚠️ Alle Dateien auf dem Server werden unwiderruflich gelöscht und anschließend die gesamte lokale Vault hochgeladen.',
		});
		contentEl.createEl('p', {
			text: 'Bist du sicher?',
			attr: { style: 'font-weight: bold;' },
		});

		const btnRow = contentEl.createDiv({ attr: { style: 'display:flex;gap:0.5rem;justify-content:flex-end;margin-top:1rem;' } });

		btnRow.createEl('button', { text: 'Abbrechen' }).onclick = () => this.close();

		const confirmBtn = btnRow.createEl('button', {
			text: 'Ja, alles löschen und hochladen',
			attr: { style: 'background:var(--color-red,#e74c3c);color:#fff;border:none;padding:0.4rem 0.8rem;border-radius:4px;cursor:pointer;' },
		});
		confirmBtn.onclick = () => {
			this.close();
			this.onConfirm();
		};
	}

	onClose() {
		this.contentEl.empty();
	}
}

function toggleFrontmatterBool(content: string, field: string): string {
	const bom = content.startsWith('﻿') ? '﻿' : '';
	const c = content.startsWith('﻿') ? content.slice(1) : content;

	if (!c.startsWith('---')) {
		// No frontmatter — prepend one
		return `${bom}---\n${field}: true\n---\n${c}`;
	}

	const afterOpen = c.slice(3);
	const endIdx = afterOpen.indexOf('\n---');
	if (endIdx === -1) {
		return `${bom}---\n${field}: true\n---\n${c.slice(3)}`;
	}

	const fm = afterOpen.slice(0, endIdx);
	const rest = afterOpen.slice(endIdx); // includes \n---

	const lines = fm.split('\n');
	const prefix = `${field}:`;
	const idx = lines.findIndex(l => l.trim().toLowerCase().startsWith(prefix.toLowerCase()));

	if (idx === -1) {
		// Field not present — add it as true
		lines.push(`${field}: true`);
	} else {
		const current = lines[idx].split(':').slice(1).join(':').trim().toLowerCase() === 'true';
		lines[idx] = `${field}: ${current ? 'false' : 'true'}`;
	}

	return `${bom}---\n${lines.join('\n')}${rest}\n`;
}

function getFrontmatterBool(content: string, field: string): boolean {
	const c = content.startsWith('﻿') ? content.slice(1) : content;
	if (!c.startsWith('---')) return false;
	const afterOpen = c.slice(3);
	const endIdx = afterOpen.indexOf('\n---');
	if (endIdx === -1) return false;
	const fm = afterOpen.slice(0, endIdx);
	const prefix = `${field}:`;
	for (const line of fm.split('\n')) {
		if (line.trim().toLowerCase().startsWith(prefix.toLowerCase())) {
			return line.split(':').slice(1).join(':').trim().toLowerCase() === 'true';
		}
	}
	return false;
}
