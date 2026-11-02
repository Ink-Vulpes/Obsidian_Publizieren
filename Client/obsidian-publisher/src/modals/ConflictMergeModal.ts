import { App, Modal } from 'obsidian';
import type { ApiClient } from '../api';
import type { SyncEngine } from '../sync-engine';

export class ConflictMergeModal extends Modal {
	private path: string;
	private api: ApiClient;
	private engine: SyncEngine;
	private onResolved: () => void;

	constructor(
		app: App,
		path: string,
		api: ApiClient,
		engine: SyncEngine,
		onResolved: () => void,
	) {
		super(app);
		this.path = path;
		this.api = api;
		this.engine = engine;
		this.onResolved = onResolved;
		this.modalEl.style.width = 'min(900px, 90vw)';
	}

	async onOpen() {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.createEl('h2', { text: `Zusammenführen: ${this.path}` });
		const loadingEl = contentEl.createEl('p', { text: 'Lade Versionen …' });

		let localBytes: ArrayBuffer;
		let serverBytes: ArrayBuffer;
		try {
			[localBytes, serverBytes] = await Promise.all([
				this.engine.readLocalFile(this.path),
				this.api.downloadFile(this.path),
			]);
		} catch (e) {
			loadingEl.setText('⚠ Fehler: ' + (e as Error).message);
			return;
		}

		const localText = new TextDecoder().decode(localBytes);
		const serverText = new TextDecoder().decode(serverBytes);
		const mergeText =
			`<<<<<<< Lokal\n${localText}\n=======\n${serverText}\n>>>>>>> Server`;

		loadingEl.remove();

		// ── Side-by-side read-only panels ────────────────────────────────────────
		const grid = contentEl.createEl('div', {
			attr: { style: 'display:grid;grid-template-columns:1fr 1fr;gap:0.75rem;margin-bottom:0.75rem;' },
		});

		const makePanel = (label: string, content: string) => {
			const wrap = grid.createEl('div');
			wrap.createEl('div', {
				text: label,
				attr: { style: 'font-weight:bold;margin-bottom:0.25rem;font-size:0.85em;opacity:0.7;' },
			});
			const ta = wrap.createEl('textarea') as HTMLTextAreaElement;
			ta.value = content;
			ta.readOnly = true;
			ta.style.cssText =
				'width:100%;height:220px;font-family:monospace;font-size:0.78em;' +
				'resize:vertical;background:var(--background-secondary);border:1px solid var(--background-modifier-border);' +
				'padding:0.4rem;border-radius:4px;color:var(--text-muted);';
			return ta;
		};

		makePanel('Lokal', localText);
		makePanel('Server', serverText);

		// ── Editable merge result ────────────────────────────────────────────────
		contentEl.createEl('div', {
			text: 'Ergebnis — Marker entfernen und Konflikte auflösen:',
			attr: { style: 'font-weight:bold;margin-bottom:0.25rem;font-size:0.85em;' },
		});
		const mergeTa = contentEl.createEl('textarea') as HTMLTextAreaElement;
		mergeTa.value = mergeText;
		mergeTa.style.cssText =
			'width:100%;height:260px;font-family:monospace;font-size:0.78em;' +
			'resize:vertical;border:1px solid var(--background-modifier-border);' +
			'padding:0.4rem;border-radius:4px;margin-bottom:0.75rem;';

		// ── Conflict marker warning ──────────────────────────────────────────────
		const warnEl = contentEl.createEl('p', {
			attr: { style: 'color:var(--color-orange,#e67e22);font-size:0.82em;display:none;margin-bottom:0.5rem;' },
		});

		const hasMarkers = (text: string) =>
			text.includes('<<<<<<<') || text.includes('=======') || text.includes('>>>>>>>');

		mergeTa.oninput = () => {
			warnEl.style.display = hasMarkers(mergeTa.value) ? 'block' : 'none';
			warnEl.setText('⚠ Conflict-Marker noch vorhanden — bitte vor dem Übernehmen vollständig auflösen.');
		};

		// ── Buttons ──────────────────────────────────────────────────────────────
		const btnRow = contentEl.createEl('div', {
			attr: { style: 'display:flex;gap:0.5rem;justify-content:flex-end;' },
		});
		btnRow.createEl('button', { text: 'Abbrechen' }).onclick = () => this.close();

		const applyBtn = btnRow.createEl('button', { text: 'Übernehmen', cls: 'mod-cta' });
		applyBtn.onclick = async () => {
			if (hasMarkers(mergeTa.value)) {
				warnEl.style.display = 'block';
				warnEl.setText('⚠ Conflict-Marker noch vorhanden — bitte vor dem Übernehmen vollständig auflösen.');
				return;
			}
			applyBtn.disabled = true;
			applyBtn.setText('Speichert …');
			try {
				const encoded = new TextEncoder().encode(mergeTa.value).buffer as ArrayBuffer;
				await this.engine.writeLocalFile(this.path, encoded);
				this.close();
				this.onResolved();
			} catch (e) {
				applyBtn.disabled = false;
				applyBtn.setText('Übernehmen');
				warnEl.style.display = 'block';
				warnEl.setText('⚠ Fehler beim Speichern: ' + (e as Error).message);
			}
		};
	}

	onClose() { this.contentEl.empty(); }
}
