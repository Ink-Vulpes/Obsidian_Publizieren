import { App, Modal, Notice } from 'obsidian';
import type { ApiClient } from '../api';
import { lineDiff, withContext } from '../utils/diff';

type Proposal = Awaited<ReturnType<ApiClient['adminListProposals']>>[number];

export class ProposalsModal extends Modal {
	private api: ApiClient;

	constructor(app: App, api: ApiClient) {
		super(app);
		this.api = api;
		this.modalEl.addClass('publisher-diff-modal');
	}

	async onOpen() {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.createEl('h2', { text: 'Änderungsanträge' });

		const loading = contentEl.createEl('p', { text: 'Lade Anträge …', cls: 'publisher-progress' });

		let proposals: Proposal[];
		try {
			proposals = await this.api.adminListProposals();
		} catch (e) {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
			return;
		}
		loading.remove();

		if (proposals.length === 0) {
			contentEl.createEl('p', { text: 'Keine offenen Anträge.', cls: 'publisher-progress' });
			return;
		}

		for (const p of proposals) {
			this.renderCard(contentEl, p);
		}
	}

	private renderCard(parent: HTMLElement, p: Proposal) {
		const card = parent.createEl('div', { cls: 'publisher-user-card' });

		// ── Header ─────────────────────────────────────────────────────────────
		const header = card.createEl('div', { cls: 'publisher-user-header' });
		header.createEl('strong', { text: p.note_slug });
		header.createEl('span', { text: `von ${p.proposer_username}`, cls: 'publisher-badge' });
		const dateStr = new Date(p.created_at).toLocaleString('de-DE', { dateStyle: 'short', timeStyle: 'short' });
		header.createEl('span', { text: dateStr, cls: 'publisher-badge' });

		if (p.message) {
			card.createEl('p', { text: `💬 ${p.message}` })
				.style.cssText = 'font-size:12px;color:var(--text-muted);margin:4px 0 6px;';
		}

		// ── Diff toggle ────────────────────────────────────────────────────────
		const diffToggle = card.createEl('button', { text: '▶ Änderungen anzeigen' });
		diffToggle.style.cssText = 'background:none;border:none;color:var(--text-muted);font-size:12px;cursor:pointer;padding:0;margin-bottom:6px;';

		const diffContainer = card.createEl('div');
		diffContainer.style.display = 'none';
		let rendered = false;

		diffToggle.onclick = async () => {
			const open = diffContainer.style.display !== 'none';
			diffContainer.style.display = open ? 'none' : 'block';
			diffToggle.setText((open ? '▶' : '▼') + ' Änderungen anzeigen');

			if (!open && !rendered) {
				rendered = true;
				diffContainer.setText('Lade aktuellen Inhalt …');
				// Fetch current content from server via sync download
				let currentText = '';
				try {
					// Try to get the file from vault (local Obsidian file)
					const slug = p.note_slug;
					// Try common paths: slug.md, slug/index.md
					const candidates = [`${slug}.md`, `${slug}/index.md`];
					for (const path of candidates) {
						const f = this.app.vault.getFileByPath(path);
						if (f) { currentText = await this.app.vault.read(f); break; }
					}
				} catch { /* ignore — diff from empty */ }
				diffContainer.empty();
				this.renderDiff(diffContainer, currentText, p.proposed_content);
			}
		};

		// ── Action buttons ─────────────────────────────────────────────────────
		const btnRow = card.createEl('div');
		btnRow.style.cssText = 'display:flex;gap:6px;margin-top:8px;';

		const approveBtn = btnRow.createEl('button', { text: 'Annehmen', cls: 'mod-cta' });
		approveBtn.style.cssText = 'font-size:12px;padding:3px 10px;';
		approveBtn.onclick = async () => {
			if (!confirm('Antrag annehmen? Der Inhalt der Note wird überschrieben.')) return;
			approveBtn.setText('…'); approveBtn.disabled = true;
			try {
				await this.api.adminApproveProposal(p.id);
				new Notice(`Antrag für „${p.note_slug}" angenommen`);
				card.remove();
			} catch (e) {
				new Notice('Fehler: ' + (e as Error).message);
				approveBtn.setText('Annehmen'); approveBtn.disabled = false;
			}
		};

		const rejectBtn = btnRow.createEl('button', { text: 'Ablehnen', cls: 'publisher-danger-btn' });
		rejectBtn.style.cssText = 'font-size:12px;padding:3px 10px;';
		rejectBtn.onclick = async () => {
			rejectBtn.setText('…'); rejectBtn.disabled = true;
			try {
				await this.api.adminRejectProposal(p.id);
				new Notice(`Antrag für „${p.note_slug}" abgelehnt`);
				card.remove();
			} catch (e) {
				new Notice('Fehler: ' + (e as Error).message);
				rejectBtn.setText('Ablehnen'); rejectBtn.disabled = false;
			}
		};
	}

	private renderDiff(parent: HTMLElement, oldText: string, newText: string) {
		if (!oldText && !newText) {
			parent.createEl('p', { text: 'Keine Inhalte verfügbar.', cls: 'publisher-progress' });
			return;
		}

		const scroll = parent.createEl('div', { cls: 'publisher-diff-scroll' });
		scroll.style.cssText = 'max-height:350px;overflow:auto;margin-bottom:4px;';
		const pre = scroll.createEl('pre', { cls: 'publisher-diff-pre' });

		if (!oldText) {
			// All new
			for (const line of newText.split('\n')) {
				const row = pre.createEl('div', { cls: 'publisher-diff-add' });
				row.createEl('span', { text: '+', cls: 'publisher-diff-marker' });
				row.createEl('span', { text: line });
			}
			return;
		}

		const diff = lineDiff(oldText, newText);
		const lines = withContext(diff);

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

	onClose() { this.contentEl.empty(); }
}
