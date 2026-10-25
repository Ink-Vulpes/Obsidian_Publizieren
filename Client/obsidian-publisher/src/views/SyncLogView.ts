import { ItemView, WorkspaceLeaf } from 'obsidian';
import type { LogStore, LogEntry } from '../log-store';

export const SYNC_LOG_VIEW_TYPE = 'obsidian-publisher-log';

function fmt(ts: number): string {
	return new Date(ts).toLocaleString('de-DE', {
		day: '2-digit', month: '2-digit', year: 'numeric',
		hour: '2-digit', minute: '2-digit', second: '2-digit',
	});
}

export class SyncLogView extends ItemView {
	private store: LogStore;
	private unsubscribe: (() => void) | null = null;

	constructor(leaf: WorkspaceLeaf, store: LogStore) {
		super(leaf);
		this.store = store;
	}

	getViewType() { return SYNC_LOG_VIEW_TYPE; }
	getDisplayText() { return 'Publisher Log'; }
	getIcon() { return 'history'; }

	async onOpen() {
		this.render();
		this.unsubscribe = this.store.onChange(() => this.render());
	}

	async onClose() {
		this.unsubscribe?.();
	}

	private render() {
		const root = this.containerEl.children[1] as HTMLElement;
		root.empty();
		root.addClass('publisher-log-root');

		// ── Header ──────────────────────────────────────────────────────────────
		const header = root.createEl('div', { cls: 'publisher-log-header' });
		header.createEl('span', { text: 'Sync-Protokoll', cls: 'publisher-log-title' });
		const clearBtn = header.createEl('button', { text: 'Leeren', cls: 'publisher-log-clear' });
		clearBtn.onclick = () => this.store.clear();

		// ── Entries ─────────────────────────────────────────────────────────────
		const entries = this.store.getEntries();
		if (entries.length === 0) {
			root.createEl('p', { text: 'Noch keine Synchronisierungen.', cls: 'publisher-log-empty' });
			return;
		}

		const list = root.createEl('div', { cls: 'publisher-log-list' });
		for (const entry of entries) {
			this.renderEntry(list, entry);
		}
	}

	private renderEntry(parent: HTMLElement, entry: LogEntry) {
		const total = entry.push.length + entry.pull.length +
			entry.deleteRemote.length + entry.deleteLocal.length;
		const hasErrors = entry.errors.length > 0;

		const card = parent.createEl('div', {
			cls: 'publisher-log-card' + (hasErrors ? ' publisher-log-card-error' : ''),
		});

		// Summary line
		const summary = card.createEl('div', { cls: 'publisher-log-summary' });
		summary.createEl('span', { text: fmt(entry.timestamp), cls: 'publisher-log-ts' });

		if (total === 0 && !hasErrors) {
			summary.createEl('span', { text: 'Keine Änderungen', cls: 'publisher-log-none' });
		} else {
			const chips = summary.createEl('span', { cls: 'publisher-log-chips' });
			if (entry.push.length)         chips.createEl('span', { text: `↑${entry.push.length}`, cls: 'publisher-chip-push' });
			if (entry.pull.length)         chips.createEl('span', { text: `↓${entry.pull.length}`, cls: 'publisher-chip-pull' });
			if (entry.deleteRemote.length) chips.createEl('span', { text: `✕S${entry.deleteRemote.length}`, cls: 'publisher-chip-del' });
			if (entry.deleteLocal.length)  chips.createEl('span', { text: `✕L${entry.deleteLocal.length}`, cls: 'publisher-chip-del' });
			if (hasErrors)                 chips.createEl('span', { text: `⚠${entry.errors.length}`, cls: 'publisher-chip-err' });
		}

		// Expandable detail
		if (total > 0 || hasErrors) {
			const detail = card.createEl('div', { cls: 'publisher-log-detail' });
			detail.style.display = 'none';

			this.renderFileList(detail, '↑ Hochgeladen', entry.push, 'publisher-log-push');
			this.renderFileList(detail, '↓ Heruntergeladen', entry.pull, 'publisher-log-pull');
			this.renderFileList(detail, '✕ Vom Server gelöscht', entry.deleteRemote, 'publisher-log-del');
			this.renderFileList(detail, '✕ Lokal gelöscht', entry.deleteLocal, 'publisher-log-del');
			if (hasErrors) {
				this.renderFileList(detail, '⚠ Fehler', entry.errors, 'publisher-log-err');
			}

			summary.style.cursor = 'pointer';
			summary.onclick = () => {
				detail.style.display = detail.style.display === 'none' ? 'block' : 'none';
			};
		}
	}

	private renderFileList(parent: HTMLElement, label: string, items: string[], cls: string) {
		if (items.length === 0) return;
		parent.createEl('div', { text: label, cls: 'publisher-log-section-label ' + cls });
		const ul = parent.createEl('ul', { cls: 'publisher-log-filelist' });
		for (const item of items) {
			ul.createEl('li', { text: item });
		}
	}
}
