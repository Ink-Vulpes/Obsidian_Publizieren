import { App, Modal } from 'obsidian';
import type { LogStore, LogEntry } from '../log-store';

function fmt(ts: number): string {
	return new Date(ts).toLocaleString('de-DE', {
		day: '2-digit', month: '2-digit', year: 'numeric',
		hour: '2-digit', minute: '2-digit', second: '2-digit',
	});
}

export class LogModal extends Modal {
	private store: LogStore;

	constructor(app: App, store: LogStore) {
		super(app);
		this.store = store;
		this.modalEl.style.minWidth = '580px';
		this.modalEl.style.maxHeight = '80vh';
	}

	onOpen() {
		const { contentEl } = this;
		contentEl.empty();

		const headerRow = contentEl.createEl('div', {
			attr: { style: 'display:flex;align-items:center;justify-content:space-between;margin-bottom:12px;' },
		});
		headerRow.createEl('h2', { text: 'Sync-Protokoll', attr: { style: 'margin:0' } });
		const clearBtn = headerRow.createEl('button', { text: 'Alles leeren' });
		clearBtn.onclick = () => {
			this.store.clear();
			this.onOpen();
		};

		const entries = this.store.getEntries();
		if (entries.length === 0) {
			contentEl.createEl('p', {
				text: 'Noch keine Synchronisierungen aufgezeichnet.',
				cls: 'publisher-log-empty',
			});
			return;
		}

		const scroll = contentEl.createEl('div', {
			attr: { style: 'overflow-y:auto;max-height:60vh;' },
		});

		for (const entry of entries) {
			this.renderEntry(scroll, entry);
		}
	}

	private renderEntry(parent: HTMLElement, entry: LogEntry) {
		const total = entry.push.length + entry.pull.length +
			entry.deleteRemote.length + entry.deleteLocal.length;
		const hasErrors = entry.errors.length > 0;

		const block = parent.createEl('div', {
			cls: 'publisher-log-card' + (hasErrors ? ' publisher-log-card-error' : ''),
			attr: { style: 'margin-bottom:10px;' },
		});

		block.createEl('div', { text: fmt(entry.timestamp), cls: 'publisher-log-ts' });

		if (total === 0 && !hasErrors) {
			block.createEl('div', { text: 'Keine Änderungen', cls: 'publisher-log-none' });
			return;
		}

		this.renderSection(block, '↑ Hochgeladen', entry.push, 'publisher-log-push');
		this.renderSection(block, '↓ Heruntergeladen', entry.pull, 'publisher-log-pull');
		this.renderSection(block, '✕ Vom Server gelöscht', entry.deleteRemote, 'publisher-log-del');
		this.renderSection(block, '✕ Lokal gelöscht', entry.deleteLocal, 'publisher-log-del');
		if (hasErrors) {
			this.renderSection(block, '⚠ Fehler', entry.errors, 'publisher-log-err');
		}
	}

	private renderSection(parent: HTMLElement, label: string, items: string[], cls: string) {
		if (items.length === 0) return;
		parent.createEl('div', { text: label, cls: 'publisher-log-section-label ' + cls });
		const ul = parent.createEl('ul', { cls: 'publisher-log-filelist' });
		for (const item of items) {
			ul.createEl('li', { text: item });
		}
	}

	onClose() {
		this.contentEl.empty();
	}
}
