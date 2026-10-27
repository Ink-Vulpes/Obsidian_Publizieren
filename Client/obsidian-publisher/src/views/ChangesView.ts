import { ItemView, Notice, WorkspaceLeaf } from 'obsidian';
import type { ApiClient } from '../api';
import type { SyncEngine, SyncDiff, ConflictResolution } from '../sync-engine';
import { FileDiffModal } from '../modals/FileDiffModal';
import type { DiffAction } from '../modals/FileDiffModal';

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
	private selected = new Set<string>();
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

	async onOpen() { await this.refresh(); }

	async refresh() {
		this.state = { kind: 'loading' };
		this.render();
		try {
			const diff = await this.engine.computeDiff();
			// Keep existing conflict resolutions, add defaults for new conflicts
			const newRes = new Map<string, ConflictResolution>();
			for (const p of diff.conflicts) newRes.set(p, this.resolutions.get(p) ?? 'push');
			this.resolutions = newRes;
			// Select everything by default on each refresh
			this.selected = new Set([
				...diff.push, ...diff.pull,
				...diff.deleteRemote, ...diff.deleteLocal, ...diff.conflicts,
			]);
			this.state = { kind: 'ready', diff };
		} catch (e) {
			this.state = { kind: 'error', message: (e as Error).message };
		}
		this.render();
	}

	// ── Render ────────────────────────────────────────────────────────────────

	private render() {
		const root = this.containerEl.children[1] as HTMLElement;

		// Save scroll before re-render
		const body = root.querySelector('.publisher-changes-body') as HTMLElement | null;
		const scrollTop = body?.scrollTop ?? 0;

		root.empty();
		root.addClass('publisher-changes-root');

		// ── Toolbar ────────────────────────────────────────────────────────────
		const toolbar = root.createEl('div', { cls: 'publisher-changes-toolbar' });
		toolbar.createEl('span', { text: 'Ausstehende Änderungen', cls: 'publisher-changes-title' });
		const right = toolbar.createEl('div', { cls: 'publisher-changes-toolbar-right' });
		const refreshBtn = right.createEl('button', { cls: 'publisher-icon-btn', attr: { title: 'Aktualisieren' } });
		refreshBtn.innerHTML = '↻';
		refreshBtn.onclick = () => this.refresh();

		// ── States ─────────────────────────────────────────────────────────────
		if (this.state.kind === 'idle') {
			root.createEl('p', { text: 'Klicke ↻ um Änderungen zu berechnen.', cls: 'publisher-changes-hint' });
			return;
		}
		if (this.state.kind === 'loading') {
			root.createEl('p', { text: 'Berechne Änderungen …', cls: 'publisher-changes-hint' });
			return;
		}
		if (this.state.kind === 'error') {
			root.createEl('p', { text: '⚠ ' + this.state.message, cls: 'publisher-changes-error' });
			return;
		}
		if (this.state.kind === 'syncing') {
			root.createEl('p', { text: this.state.progress, cls: 'publisher-changes-hint' });
			return;
		}

		// ── Ready ──────────────────────────────────────────────────────────────
		const { diff } = this.state;
		const allPaths = [
			...diff.push, ...diff.pull,
			...diff.deleteRemote, ...diff.deleteLocal, ...diff.conflicts,
		];

		if (allPaths.length === 0) {
			root.createEl('p', { text: '✓ Vault ist synchronisiert.', cls: 'publisher-changes-synced' });
			return;
		}

		// Global select-all
		const globalRow = root.createEl('div', { cls: 'publisher-changes-select-all-row' });
		const globalCb = globalRow.createEl('input', { cls: 'publisher-changes-cb' }) as HTMLInputElement;
		globalCb.type = 'checkbox';
		this.setTristate(globalCb, allPaths);
		globalRow.createEl('span', { text: `Alle (${this.selected.size}/${allPaths.length})`, cls: 'publisher-changes-select-all-label' });
		globalCb.onchange = () => {
			if (globalCb.checked) allPaths.forEach(p => this.selected.add(p));
			else allPaths.forEach(p => this.selected.delete(p));
			this.rerender(scrollTop);
		};

		// Scrollable body
		const bodyEl = root.createEl('div', { cls: 'publisher-changes-body' });

		this.renderSection(bodyEl, `↑ Hochladen`, diff.push, 'publisher-changes-push', 'push');
		this.renderSection(bodyEl, `↓ Herunterladen`, diff.pull, 'publisher-changes-pull', 'pull');
		this.renderSection(bodyEl, `✕ Vom Server löschen`, diff.deleteRemote, 'publisher-changes-del', 'deleteRemote');
		this.renderSection(bodyEl, `✕ Lokal löschen`, diff.deleteLocal, 'publisher-changes-del', 'deleteLocal');
		this.renderConflicts(bodyEl, diff.conflicts);

		// Footer
		const footer = root.createEl('div', { cls: 'publisher-changes-footer' });
		const nSelected = this.selected.size;
		const syncBtn = footer.createEl('button', {
			text: `Synchronisieren (${nSelected})`,
			cls: 'mod-cta publisher-changes-sync-btn',
		});
		syncBtn.disabled = nSelected === 0;
		syncBtn.onclick = () => this.doSync(diff);

		// Restore scroll
		window.setTimeout(() => { bodyEl.scrollTop = scrollTop; }, 0);
	}

	// ── Section rendering ─────────────────────────────────────────────────────

	private renderSection(parent: HTMLElement, label: string, paths: string[], cls: string, action: DiffAction) {
		if (paths.length === 0) return;
		const key = 'publisher-section-collapsed-' + cls;
		const collapsed = (() => { try { return localStorage.getItem(key) === '1'; } catch { return false; } })();

		const section = parent.createEl('div', { cls: 'publisher-changes-section' });

		// Section header with select-all checkbox + collapse toggle
		const header = section.createEl('div', { cls: 'publisher-changes-section-header' });
		const sectionCb = header.createEl('input', { cls: 'publisher-changes-cb' }) as HTMLInputElement;
		sectionCb.type = 'checkbox';
		this.setTristate(sectionCb, paths);
		sectionCb.onchange = () => {
			if (sectionCb.checked) paths.forEach(p => this.selected.add(p));
			else paths.forEach(p => this.selected.delete(p));
			this.rerender();
		};
		const labelEl = header.createEl('span', { text: `${label} (${paths.length})`, cls: `publisher-changes-section-label ${cls}` });
		const arrow = header.createEl('span', { cls: 'publisher-changes-collapse-arrow', text: collapsed ? '▶' : '▼' });

		// Toggle on label/arrow click (not on checkbox)
		const toggle = () => {
			const nowCollapsed = ul.style.display !== 'none';
			ul.style.display = nowCollapsed ? 'none' : '';
			arrow.textContent = nowCollapsed ? '▶' : '▼';
			try { localStorage.setItem(key, nowCollapsed ? '1' : '0'); } catch { /* */ }
		};
		labelEl.style.cursor = 'pointer';
		labelEl.onclick = toggle;
		arrow.onclick = toggle;

		// File list
		const ul = section.createEl('ul', { cls: 'publisher-changes-filelist' });
		if (collapsed) ul.style.display = 'none';

		for (const p of paths) {
			const li = ul.createEl('li', { cls: 'publisher-changes-file-item' });
			const cb = li.createEl('input', { cls: 'publisher-changes-cb' }) as HTMLInputElement;
			cb.type = 'checkbox';
			cb.checked = this.selected.has(p);
			cb.onchange = () => {
				if (cb.checked) this.selected.add(p);
				else this.selected.delete(p);
				this.rerender();
			};
			const link = li.createEl('span', { text: p, cls: 'publisher-changes-file-link' });
			link.onclick = () => new FileDiffModal(this.app, p, action, this.api).open();
		}
	}

	private renderConflicts(parent: HTMLElement, paths: string[]) {
		if (paths.length === 0) return;
		const key = 'publisher-section-collapsed-conflicts';
		const collapsed = (() => { try { return localStorage.getItem(key) === '1'; } catch { return false; } })();

		const section = parent.createEl('div', { cls: 'publisher-changes-section' });

		const header = section.createEl('div', { cls: 'publisher-changes-section-header' });
		const sectionCb = header.createEl('input', { cls: 'publisher-changes-cb' }) as HTMLInputElement;
		sectionCb.type = 'checkbox';
		this.setTristate(sectionCb, paths);
		sectionCb.onchange = () => {
			if (sectionCb.checked) paths.forEach(p => this.selected.add(p));
			else paths.forEach(p => this.selected.delete(p));
			this.rerender();
		};
		const labelEl = header.createEl('span', {
			text: `⚠ Konflikte (${paths.length}) — beide Seiten geändert`,
			cls: 'publisher-changes-section-label publisher-changes-conflict',
		});
		const arrow = header.createEl('span', { cls: 'publisher-changes-collapse-arrow', text: collapsed ? '▶' : '▼' });

		const body = section.createEl('div');
		if (collapsed) body.style.display = 'none';

		const toggle = () => {
			const nowCollapsed = body.style.display !== 'none';
			body.style.display = nowCollapsed ? 'none' : '';
			arrow.textContent = nowCollapsed ? '▶' : '▼';
			try { localStorage.setItem(key, nowCollapsed ? '1' : '0'); } catch { /* */ }
		};
		labelEl.style.cursor = 'pointer';
		labelEl.onclick = toggle;
		arrow.onclick = toggle;

		for (const p of paths) {
			const row = body.createEl('div', { cls: 'publisher-changes-conflict-row' });

			const cb = row.createEl('input', { cls: 'publisher-changes-cb' }) as HTMLInputElement;
			cb.type = 'checkbox';
			cb.checked = this.selected.has(p);
			cb.onchange = () => {
				if (cb.checked) this.selected.add(p);
				else this.selected.delete(p);
				this.rerender();
			};

			const pathSpan = row.createEl('span', { text: p, cls: 'publisher-changes-conflict-path publisher-changes-file-link' });
			pathSpan.onclick = () => new FileDiffModal(this.app, p, 'conflict', this.api).open();

			const btns = row.createEl('div', { cls: 'publisher-changes-conflict-btns' });
			const current = this.resolutions.get(p) ?? 'push';
			const localBtn  = btns.createEl('button', { text: 'Lokal',  cls: 'publisher-conflict-btn' + (current === 'push' ? ' active' : '') });
			const serverBtn = btns.createEl('button', { text: 'Server', cls: 'publisher-conflict-btn' + (current === 'pull' ? ' active' : '') });
			const skipBtn   = btns.createEl('button', { text: 'Skip',   cls: 'publisher-conflict-btn' + (current === 'skip' ? ' active' : '') });

			const setRes = (res: ConflictResolution, active: HTMLButtonElement) => {
				this.resolutions.set(p, res);
				[localBtn, serverBtn, skipBtn].forEach(b => b.removeClass('active'));
				active.addClass('active');
			};
			localBtn.onclick  = () => setRes('push', localBtn);
			serverBtn.onclick = () => setRes('pull', serverBtn);
			skipBtn.onclick   = () => setRes('skip', skipBtn);
		}
	}

	// ── Helpers ───────────────────────────────────────────────────────────────

	private setTristate(cb: HTMLInputElement, paths: string[]) {
		const n = paths.filter(p => this.selected.has(p)).length;
		if (n === 0)            { cb.checked = false; cb.indeterminate = false; }
		else if (n === paths.length) { cb.checked = true;  cb.indeterminate = false; }
		else                    { cb.checked = false; cb.indeterminate = true; }
	}

	private rerender(scrollTop = 0) {
		const body = (this.containerEl.children[1] as HTMLElement)
			.querySelector('.publisher-changes-body') as HTMLElement | null;
		const s = scrollTop || body?.scrollTop || 0;
		this.render();
		window.setTimeout(() => {
			const b = (this.containerEl.children[1] as HTMLElement)
				.querySelector('.publisher-changes-body') as HTMLElement | null;
			if (b) b.scrollTop = s;
		}, 0);
	}

	// ── Sync ─────────────────────────────────────────────────────────────────

	private async doSync(diff: SyncDiff) {
		const filtered: SyncDiff = {
			push: diff.push.filter(p => this.selected.has(p)),
			pull: diff.pull.filter(p => this.selected.has(p)),
			deleteRemote: diff.deleteRemote.filter(p => this.selected.has(p)),
			deleteLocal: diff.deleteLocal.filter(p => this.selected.has(p)),
			conflicts: diff.conflicts.filter(p => this.selected.has(p)),
		};

		this.state = { kind: 'syncing', progress: 'Starte Sync …' };
		this.render();
		try {
			const { errors } = await this.engine.executeActions(
				filtered,
				this.resolutions,
				(msg, done, total) => {
					this.state = { kind: 'syncing', progress: `[${done}/${total}] ${msg}` };
					this.render();
				},
			);
			try { await this.api.reloadVault(); } catch { /* non-critical */ }
			if (errors.length > 0) new Notice(`Sync: ${errors.length} Fehler — ${errors[0]}`);
			else new Notice('Synchronisierung abgeschlossen.');
			this.onSyncDone?.();
			await this.refresh();
		} catch (e) {
			this.state = { kind: 'error', message: (e as Error).message };
			this.render();
		}
	}

	async onClose() { /* nothing */ }
}
