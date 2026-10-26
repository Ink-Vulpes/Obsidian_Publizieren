import { App, Modal } from 'obsidian';
import type { SyncDiff, SyncEngine, ConflictResolution } from '../sync-engine';

const MAX_PREVIEW = 30;

export class SyncModal extends Modal {
	private diff: SyncDiff;
	private engine: SyncEngine;
	private onDone: (errors: string[]) => void;
	private resolutions = new Map<string, ConflictResolution>();

	constructor(
		app: App,
		diff: SyncDiff,
		engine: SyncEngine,
		onDone: (errors: string[]) => void,
	) {
		super(app);
		this.diff = diff;
		this.engine = engine;
		this.onDone = onDone;
		// default: local wins
		for (const p of diff.conflicts) this.resolutions.set(p, 'push');
	}

	onOpen() {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.createEl('h2', { text: 'Synchronisierung' });

		const total =
			this.diff.push.length + this.diff.pull.length +
			this.diff.deleteRemote.length + this.diff.deleteLocal.length +
			this.diff.conflicts.length;

		if (total === 0) {
			contentEl.createEl('p', { text: 'Vault ist bereits synchronisiert.', cls: 'publisher-ok' });
			contentEl.createEl('button', { text: 'Schließen' }).onclick = () => this.close();
			return;
		}

		this.renderSection(contentEl, '↑ Hochladen', this.diff.push, 'publisher-push');
		this.renderSection(contentEl, '↓ Herunterladen', this.diff.pull, 'publisher-pull');
		this.renderSection(contentEl, '✕ Vom Server löschen', this.diff.deleteRemote, 'publisher-delete');
		this.renderSection(contentEl, '✕ Lokal löschen', this.diff.deleteLocal, 'publisher-delete');
		this.renderConflicts(contentEl, this.diff.conflicts);

		const progressEl = contentEl.createEl('div', { cls: 'publisher-progress' });
		progressEl.style.display = 'none';

		const btnRow = contentEl.createEl('div', { attr: { style: 'display:flex;gap:8px;margin-top:12px;' } });
		const syncBtn = btnRow.createEl('button', { text: `Synchronisieren (${total})`, cls: 'mod-cta' });
		const cancelBtn = btnRow.createEl('button', { text: 'Abbrechen' });
		cancelBtn.onclick = () => this.close();

		syncBtn.onclick = async () => {
			syncBtn.disabled = true;
			cancelBtn.disabled = true;
			progressEl.style.display = 'block';

			const { errors } = await this.engine.executeActions(
				this.diff,
				this.resolutions,
				(msg, done, total) => { progressEl.setText(`[${done}/${total}] ${msg}`); },
			);

			this.close();
			this.onDone(errors);
		};
	}

	private renderSection(parent: HTMLElement, label: string, paths: string[], cls: string) {
		if (paths.length === 0) return;
		parent.createEl('h4', { text: label });
		const ul = parent.createEl('ul', { cls: `publisher-list ${cls}` });
		const shown = paths.slice(0, MAX_PREVIEW);
		for (const p of shown) ul.createEl('li', { text: p });
		if (paths.length > MAX_PREVIEW) {
			ul.createEl('li', { text: `… und ${paths.length - MAX_PREVIEW} weitere`, cls: 'publisher-more' });
		}
	}

	private renderConflicts(parent: HTMLElement, paths: string[]) {
		if (paths.length === 0) return;
		parent.createEl('h4', { text: '⚠ Konflikte — beide Seiten geändert' });
		for (const p of paths.slice(0, MAX_PREVIEW)) {
			const row = parent.createEl('div', { cls: 'publisher-conflict-modal-row' });
			row.createEl('span', { text: p, cls: 'publisher-conflict-modal-path' });
			const btns = row.createEl('span', { cls: 'publisher-conflict-modal-btns' });

			const localBtn = btns.createEl('button', { text: 'Lokal', cls: 'publisher-conflict-btn active' });
			const serverBtn = btns.createEl('button', { text: 'Server', cls: 'publisher-conflict-btn' });

			localBtn.onclick = () => {
				this.resolutions.set(p, 'push');
				localBtn.addClass('active'); serverBtn.removeClass('active');
			};
			serverBtn.onclick = () => {
				this.resolutions.set(p, 'pull');
				serverBtn.addClass('active'); localBtn.removeClass('active');
			};
		}
		if (paths.length > MAX_PREVIEW) {
			parent.createEl('p', { text: `… und ${paths.length - MAX_PREVIEW} weitere (Lokal gewinnt)`, cls: 'publisher-more' });
		}
	}

	onClose() { this.contentEl.empty(); }
}
