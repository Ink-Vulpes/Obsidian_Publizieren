import { ItemView, Notice, WorkspaceLeaf } from 'obsidian';
import type { ApiClient } from '../api';
import type { SyncEngine, SyncDiff, ConflictResolution } from '../sync-engine';

export const CHANGES_VIEW_TYPE = 'obsidian-publisher-changes';

type State =
	| { kind: 'idle' }
	| { kind: 'loading' }
	| { kind: 'ready'; diff: SyncDiff }
	| { kind: 'syncing'; progress: string }
	| { kind: 'error'; message: string };

export class ChangesView extends ItemView {
	private engine: SyncEngine;
	private api: ApiClient;
	private state: State = { kind: 'idle' };
	private resolutions = new Map<string, ConflictResolution>();
	private onSyncDone: (() => void) | null = null;

	constructor(leaf: WorkspaceLeaf, engine: SyncEngine, api: ApiClient) {
		super(leaf);
		this.engine = engine;
		this.api = api;
	}

	getViewType() { return CHANGES_VIEW_TYPE; }
	getDisplayText() { return 'Ausstehende Änderungen'; }
	getIcon() { return 'git-compare'; }

	setOnSyncDone(fn: () => void) { this.onSyncDone = fn; }

	async onOpen() {
		await this.refresh();
	}

	async refresh() {
		this.state = { kind: 'loading' };
		this.render();
		try {
			const diff = await this.engine.computeDiff();
			// Reset only conflict resolutions for paths no longer in conflicts
			const newResolutions = new Map<string, ConflictResolution>();
			for (const p of diff.conflicts) {
				newResolutions.set(p, this.resolutions.get(p) ?? 'push');
			}
			this.resolutions = newResolutions;
			this.state = { kind: 'ready', diff };
		} catch (e) {
			this.state = { kind: 'error', message: (e as Error).message };
		}
		this.render();
	}

	private render() {
		const root = this.containerEl.children[1] as HTMLElement;
		root.empty();
		root.addClass('publisher-changes-root');

		// ── Toolbar ─────────────────────────────────────────────────────────────
		const toolbar = root.createEl('div', { cls: 'publisher-changes-toolbar' });
		toolbar.createEl('span', { text: 'Ausstehende Änderungen', cls: 'publisher-changes-title' });

		const refreshBtn = toolbar.createEl('button', { cls: 'publisher-icon-btn', attr: { title: 'Aktualisieren' } });
		refreshBtn.innerHTML = '↻';
		refreshBtn.onclick = () => this.refresh();

		// ── Body ────────────────────────────────────────────────────────────────
		if (this.state.kind === 'idle') {
			root.createEl('p', { text: 'Klicke ↻ um Änderungen zu berechnen.', cls: 'publisher-changes-hint' });
			return;
		}

		if (this.state.kind === 'loading') {
			root.createEl('p', { text: 'Berechne Änderungen …', cls: 'publisher-changes-hint' });
			return;
		}

		if (this.state.kind === 'error') {
			root.createEl('p', {
				text: '⚠ ' + this.state.message,
				cls: 'publisher-changes-error',
			});
			return;
		}

		if (this.state.kind === 'syncing') {
			root.createEl('p', { text: this.state.progress, cls: 'publisher-changes-hint' });
			return;
		}

		// ── Ready ────────────────────────────────────────────────────────────────
		const { diff } = this.state;
		const totalNonConflict = diff.push.length + diff.pull.length +
			diff.deleteRemote.length + diff.deleteLocal.length;
		const total = totalNonConflict + diff.conflicts.length;

		if (total === 0) {
			root.createEl('p', { text: '✓ Vault ist synchronisiert.', cls: 'publisher-changes-synced' });
		} else {
			this.renderSection(root, `↑ Hochladen (${diff.push.length})`, diff.push, 'publisher-changes-push');
			this.renderSection(root, `↓ Herunterladen (${diff.pull.length})`, diff.pull, 'publisher-changes-pull');
			this.renderSection(root, `✕ Vom Server löschen (${diff.deleteRemote.length})`, diff.deleteRemote, 'publisher-changes-del');
			this.renderSection(root, `✕ Lokal löschen (${diff.deleteLocal.length})`, diff.deleteLocal, 'publisher-changes-del');
			this.renderConflicts(root, diff.conflicts);
		}

		// ── Sync button ─────────────────────────────────────────────────────────
		const footer = root.createEl('div', { cls: 'publisher-changes-footer' });
		if (total > 0) {
			const syncBtn = footer.createEl('button', {
				text: `Synchronisieren (${total})`,
				cls: 'mod-cta publisher-changes-sync-btn',
			});
			syncBtn.onclick = () => this.doSync(diff);
		}
	}

	private renderSection(parent: HTMLElement, label: string, paths: string[], cls: string) {
		if (paths.length === 0) return;
		const section = parent.createEl('div', { cls: 'publisher-changes-section' });
		section.createEl('div', { text: label, cls: `publisher-changes-section-label ${cls}` });
		const ul = section.createEl('ul', { cls: 'publisher-changes-filelist' });
		for (const p of paths) ul.createEl('li', { text: p });
	}

	private renderConflicts(parent: HTMLElement, paths: string[]) {
		if (paths.length === 0) return;
		const section = parent.createEl('div', { cls: 'publisher-changes-section' });
		section.createEl('div', {
			text: `⚠ Konflikte (${paths.length}) — beide Seiten geändert`,
			cls: 'publisher-changes-section-label publisher-changes-conflict',
		});

		for (const p of paths) {
			const row = section.createEl('div', { cls: 'publisher-changes-conflict-row' });
			row.createEl('span', { text: p, cls: 'publisher-changes-conflict-path' });

			const btns = row.createEl('div', { cls: 'publisher-changes-conflict-btns' });
			const current = this.resolutions.get(p) ?? 'push';

			const localBtn = btns.createEl('button', {
				text: 'Lokal',
				cls: 'publisher-conflict-btn' + (current === 'push' ? ' active' : ''),
			});
			const serverBtn = btns.createEl('button', {
				text: 'Server',
				cls: 'publisher-conflict-btn' + (current === 'pull' ? ' active' : ''),
			});
			const skipBtn = btns.createEl('button', {
				text: 'Skip',
				cls: 'publisher-conflict-btn' + (current === 'skip' ? ' active' : ''),
			});

			localBtn.onclick = () => {
				this.resolutions.set(p, 'push');
				[localBtn, serverBtn, skipBtn].forEach(b => b.removeClass('active'));
				localBtn.addClass('active');
			};
			serverBtn.onclick = () => {
				this.resolutions.set(p, 'pull');
				[localBtn, serverBtn, skipBtn].forEach(b => b.removeClass('active'));
				serverBtn.addClass('active');
			};
			skipBtn.onclick = () => {
				this.resolutions.set(p, 'skip');
				[localBtn, serverBtn, skipBtn].forEach(b => b.removeClass('active'));
				skipBtn.addClass('active');
			};
		}
	}

	private async doSync(diff: SyncDiff) {
		this.state = { kind: 'syncing', progress: 'Starte Sync …' };
		this.render();
		try {
			const { errors } = await this.engine.executeActions(
				diff,
				this.resolutions,
				(msg, done, total) => {
					this.state = { kind: 'syncing', progress: `[${done}/${total}] ${msg}` };
					this.render();
				},
			);
			try { await this.api.reloadVault(); } catch { /* non-critical */ }
			if (errors.length > 0) {
				new Notice(`Sync: ${errors.length} Fehler — ${errors[0]}`);
			} else {
				new Notice('Synchronisierung abgeschlossen.');
			}
			this.onSyncDone?.();
			await this.refresh();
		} catch (e) {
			this.state = { kind: 'error', message: (e as Error).message };
			this.render();
		}
	}

	async onClose() { /* nothing */ }
}
