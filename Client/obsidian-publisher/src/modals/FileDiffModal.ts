import { App, Modal } from 'obsidian';
import type { ApiClient } from '../api';
import { lineDiff, withContext, isTextFile } from '../utils/diff';

export type DiffAction = 'push' | 'pull' | 'deleteRemote' | 'deleteLocal' | 'conflict';

function label(action: DiffAction): { left: string; right: string; badge: string; badgeCls: string } {
	switch (action) {
		case 'push':         return { left: 'Lokal (neu)',   right: 'Server (alt)',  badge: '↑ Hochladen',         badgeCls: 'publisher-diff-badge-push' };
		case 'pull':         return { left: 'Server (neu)',  right: 'Lokal (alt)',   badge: '↓ Herunterladen',     badgeCls: 'publisher-diff-badge-pull' };
		case 'deleteRemote': return { left: 'Lokal',        right: 'Server',        badge: '✕ Vom Server löschen', badgeCls: 'publisher-diff-badge-del' };
		case 'deleteLocal':  return { left: 'Server',       right: 'Lokal',         badge: '✕ Lokal löschen',     badgeCls: 'publisher-diff-badge-del' };
		case 'conflict':     return { left: 'Lokal',        right: 'Server',        badge: '⚠ Konflikt',          badgeCls: 'publisher-diff-badge-conflict' };
	}
}

export class FileDiffModal extends Modal {
	private path: string;
	private action: DiffAction;
	private api: ApiClient;

	constructor(app: App, path: string, action: DiffAction, api: ApiClient) {
		super(app);
		this.path = path;
		this.action = action;
		this.api = api;
		this.modalEl.addClass('publisher-diff-modal');
	}

	async onOpen() {
		const { contentEl } = this;
		contentEl.empty();

		// ── Header ──────────────────────────────────────────────────────────────
		const header = contentEl.createEl('div', { cls: 'publisher-diff-header' });
		const meta = label(this.action);
		header.createEl('span', { text: meta.badge, cls: `publisher-diff-badge ${meta.badgeCls}` });
		header.createEl('code', { text: this.path, cls: 'publisher-diff-path' });

		contentEl.createEl('div', { text: 'Lade …', cls: 'publisher-diff-loading' });

		// ── Fetch both sides ─────────────────────────────────────────────────────
		let localText: string | null = null;
		let serverText: string | null = null;

		try {
			const file = this.app.vault.getFileByPath(this.path);
			if (file) {
				const buf = await this.app.vault.readBinary(file);
				localText = isTextFile(this.path)
					? new TextDecoder().decode(buf)
					: `[Binärdatei — ${buf.byteLength} Bytes]`;
			}
		} catch { /* file may not exist locally */ }

		try {
			const buf = await this.api.downloadFile(this.path);
			serverText = isTextFile(this.path)
				? new TextDecoder().decode(buf)
				: `[Binärdatei — ${buf.byteLength} Bytes]`;
		} catch { /* file may not exist on server */ }

		// remove loading indicator
		contentEl.querySelector('.publisher-diff-loading')?.remove();

		// ── Render ───────────────────────────────────────────────────────────────
		const scroll = contentEl.createEl('div', { cls: 'publisher-diff-scroll' });

		if (this.action === 'conflict') {
			this.renderSideBySide(scroll, localText ?? '', serverText ?? '', meta);
		} else {
			// unified diff: "from" is the old state, "to" is the new state
			const [oldText, newText] = this.resolveOldNew(localText, serverText);
			this.renderUnified(scroll, oldText, newText, meta);
		}
	}

	private resolveOldNew(local: string | null, server: string | null): [string, string] {
		switch (this.action) {
			case 'push':         return [server ?? '', local ?? ''];
			case 'pull':         return [local ?? '', server ?? ''];
			case 'deleteRemote': return [server ?? '', ''];
			case 'deleteLocal':  return [local ?? '', ''];
			default:             return [local ?? '', server ?? ''];
		}
	}

	private renderUnified(
		parent: HTMLElement,
		oldText: string,
		newText: string,
		meta: ReturnType<typeof label>,
	) {
		if (!oldText && !newText) {
			parent.createEl('p', { text: 'Keine Inhalte verfügbar.', cls: 'publisher-diff-empty' });
			return;
		}
		if (!oldText) {
			this.renderAllNew(parent, newText);
			return;
		}
		if (!newText) {
			this.renderAllOld(parent, oldText);
			return;
		}

		const diff = lineDiff(oldText, newText);
		const lines = withContext(diff);
		const pre = parent.createEl('pre', { cls: 'publisher-diff-pre' });

		for (const line of lines) {
			if (line.type === 'gap') {
				pre.createEl('div', {
					text: `@@ … ${(line as any).count} unveränderte Zeilen … @@`,
					cls: 'publisher-diff-hunk',
				});
				continue;
			}
			const cls =
				line.type === 'insert' ? 'publisher-diff-add' :
				line.type === 'delete' ? 'publisher-diff-del' :
				'publisher-diff-eq';
			const marker = line.type === 'insert' ? '+' : line.type === 'delete' ? '−' : ' ';
			const row = pre.createEl('div', { cls });
			row.createEl('span', { text: marker, cls: 'publisher-diff-marker' });
			row.createEl('span', { text: line.text });
		}
	}

	private renderAllNew(parent: HTMLElement, text: string) {
		const pre = parent.createEl('pre', { cls: 'publisher-diff-pre' });
		for (const line of text.split('\n')) {
			const row = pre.createEl('div', { cls: 'publisher-diff-add' });
			row.createEl('span', { text: '+', cls: 'publisher-diff-marker' });
			row.createEl('span', { text: line });
		}
	}

	private renderAllOld(parent: HTMLElement, text: string) {
		const pre = parent.createEl('pre', { cls: 'publisher-diff-pre' });
		for (const line of text.split('\n')) {
			const row = pre.createEl('div', { cls: 'publisher-diff-del' });
			row.createEl('span', { text: '−', cls: 'publisher-diff-marker' });
			row.createEl('span', { text: line });
		}
	}

	private renderSideBySide(
		parent: HTMLElement,
		localText: string,
		serverText: string,
		meta: ReturnType<typeof label>,
	) {
		const wrap = parent.createEl('div', { cls: 'publisher-diff-side-wrap' });

		const leftCol = wrap.createEl('div', { cls: 'publisher-diff-side' });
		leftCol.createEl('div', { text: meta.left, cls: 'publisher-diff-side-label publisher-diff-side-local' });
		const leftPre = leftCol.createEl('pre', { cls: 'publisher-diff-pre' });

		const rightCol = wrap.createEl('div', { cls: 'publisher-diff-side' });
		rightCol.createEl('div', { text: meta.right, cls: 'publisher-diff-side-label publisher-diff-side-server' });
		const rightPre = rightCol.createEl('pre', { cls: 'publisher-diff-pre' });

		const diff = lineDiff(serverText, localText); // local as "new", server as "old"
		const lines = withContext(diff);

		for (const line of lines) {
			if (line.type === 'gap') {
				const gapTxt = `@@ … ${(line as any).count} … @@`;
				leftPre.createEl('div', { text: gapTxt, cls: 'publisher-diff-hunk' });
				rightPre.createEl('div', { text: gapTxt, cls: 'publisher-diff-hunk' });
				continue;
			}
			if (line.type === 'delete') {
				// "old" = server, show in right col
				const row = rightPre.createEl('div', { cls: 'publisher-diff-del' });
				row.createEl('span', { text: '−', cls: 'publisher-diff-marker' });
				row.createEl('span', { text: line.text });
				leftPre.createEl('div', { cls: 'publisher-diff-blank' });
			} else if (line.type === 'insert') {
				// "new" = local, show in left col
				const row = leftPre.createEl('div', { cls: 'publisher-diff-add' });
				row.createEl('span', { text: '+', cls: 'publisher-diff-marker' });
				row.createEl('span', { text: line.text });
				rightPre.createEl('div', { cls: 'publisher-diff-blank' });
			} else {
				const addRow = (pre: HTMLElement) => {
					const row = pre.createEl('div', { cls: 'publisher-diff-eq' });
					row.createEl('span', { text: ' ', cls: 'publisher-diff-marker' });
					row.createEl('span', { text: line.text });
				};
				addRow(leftPre);
				addRow(rightPre);
			}
		}
	}

	onClose() { this.contentEl.empty(); }
}
