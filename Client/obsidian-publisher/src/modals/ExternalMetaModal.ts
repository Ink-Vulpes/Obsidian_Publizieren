import { App, Modal, Notice, Setting } from 'obsidian';
import type { ApiClient } from '../api';

interface NoteItem {
	slug: string;
	title: string;
	kind: string;
}

export class ExternalMetaModal extends Modal {
	private api: ApiClient;
	private notes: NoteItem[] = [];
	private allMeta: Record<string, { groups?: string[] }> = {};
	private filter = '';
	private selectedSlug: string | null = null;
	private groupInput: HTMLInputElement | null = null;
	private statusEl: HTMLElement | null = null;

	constructor(app: App, api: ApiClient) {
		super(app);
		this.api = api;
	}

	async onOpen() {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.addClass('ext-meta-modal');
		contentEl.createEl('h2', { text: 'Externe Notiz-Metadaten' });
		contentEl.createEl('p', {
			text: 'Server-seitige Metadaten überschreiben die Frontmatter der Notiz (nützlich für .base und andere Dateien ohne Frontmatter).',
			attr: { style: 'color:var(--text-muted);font-size:0.85rem;margin-bottom:1rem;' },
		});

		const loadingEl = contentEl.createEl('div', { text: '⏳ Lade Daten…', attr: { style: 'color:var(--text-muted);' } });

		try {
			[this.notes, this.allMeta] = await Promise.all([
				this.api.adminGetNoteList(),
				this.api.adminGetAllNoteMeta(),
			]);
		} catch (e) {
			loadingEl.setText('❌ Fehler: ' + (e as Error).message);
			return;
		}
		loadingEl.remove();

		this.renderContent(contentEl);
	}

	private renderContent(contentEl: HTMLElement) {
		// Search
		const searchRow = contentEl.createDiv({ attr: { style: 'display:flex;gap:0.5rem;margin-bottom:0.75rem;' } });
		const searchInput = searchRow.createEl('input', {
			type: 'text',
			placeholder: 'Notiz suchen…',
			attr: { style: 'flex:1;background:var(--background-modifier-form-field);border:1px solid var(--background-modifier-border);border-radius:4px;padding:0.4rem 0.6rem;color:var(--text-normal);font-size:0.9rem;' },
		});
		searchInput.addEventListener('input', () => {
			this.filter = searchInput.value.toLowerCase();
			this.refreshList(listEl);
		});

		// Note list
		const listEl = contentEl.createDiv({ attr: { style: 'max-height:200px;overflow-y:auto;border:1px solid var(--background-modifier-border);border-radius:4px;margin-bottom:1rem;' } });
		this.refreshList(listEl);

		// Edit section
		const editSection = contentEl.createDiv({ attr: { style: 'border-top:1px solid var(--background-modifier-border);padding-top:0.75rem;' } });
		this.selectedSlug = null;
		this.renderEditSection(editSection);
	}

	private refreshList(listEl: HTMLElement) {
		listEl.empty();
		const filtered = this.notes.filter(n =>
			n.title.toLowerCase().includes(this.filter) ||
			n.slug.toLowerCase().includes(this.filter)
		);
		if (filtered.length === 0) {
			listEl.createDiv({ text: 'Keine Notizen gefunden', attr: { style: 'padding:0.5rem;color:var(--text-muted);font-size:0.85rem;' } });
			return;
		}
		for (const note of filtered) {
			const hasMeta = !!(this.allMeta[note.slug]?.groups?.length);
			const row = listEl.createDiv({
				attr: {
					style: `display:flex;align-items:center;gap:0.5rem;padding:0.35rem 0.5rem;cursor:pointer;border-bottom:1px solid var(--background-modifier-border-hover);${this.selectedSlug === note.slug ? 'background:var(--background-modifier-hover);' : ''}`,
				},
			});
			const kindIcon = note.kind === 'base' ? '🗃' : note.kind === 'canvas' ? '🖼' : note.kind === 'excalidraw' ? '✏️' : '📄';
			row.createSpan({ text: kindIcon, attr: { style: 'font-size:0.9rem;' } });
			const nameEl = row.createSpan({ text: note.title, attr: { style: 'flex:1;font-size:0.85rem;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;' } });
			if (hasMeta) {
				const groups = this.allMeta[note.slug]?.groups?.join(', ') ?? '';
				nameEl.setAttribute('title', note.slug + '\n🔒 ' + groups);
				row.createSpan({ text: '🔒', attr: { style: 'font-size:0.75rem;color:var(--text-accent);', title: groups } });
			} else {
				nameEl.setAttribute('title', note.slug);
			}
			row.addEventListener('click', () => {
				this.selectedSlug = note.slug;
				// Highlight selected
				listEl.querySelectorAll('div').forEach((el: HTMLElement) => el.style.background = '');
				(row as HTMLElement).style.background = 'var(--background-modifier-hover)';
				this.updateEditSection();
			});
			row.addEventListener('mouseover', () => {
				if (this.selectedSlug !== note.slug) (row as HTMLElement).style.background = 'var(--background-modifier-hover)';
			});
			row.addEventListener('mouseout', () => {
				if (this.selectedSlug !== note.slug) (row as HTMLElement).style.background = '';
			});
		}
	}

	private editSectionEl: HTMLElement | null = null;

	private renderEditSection(container: HTMLElement) {
		this.editSectionEl = container;
		this.updateEditSection();
	}

	private updateEditSection() {
		const el = this.editSectionEl;
		if (!el) return;
		el.empty();

		if (!this.selectedSlug) {
			el.createDiv({ text: 'Wähle oben eine Notiz aus.', attr: { style: 'color:var(--text-muted);font-size:0.85rem;padding:0.25rem 0;' } });
			return;
		}

		const slug = this.selectedSlug;
		const currentGroups = (this.allMeta[slug]?.groups ?? []).join(', ');

		el.createEl('div', { text: `Notiz: ${slug}`, attr: { style: 'font-size:0.8rem;color:var(--text-muted);margin-bottom:0.5rem;word-break:break-all;' } });

		new Setting(el)
			.setName('Gruppen')
			.setDesc('Kommagetrennte Gruppennamen. Leer lassen für keine Einschränkung.')
			.addText(text => {
				text.setValue(currentGroups).setPlaceholder('gruppe1, gruppe2');
				this.groupInput = text.inputEl;
				this.groupInput.style.width = '200px';
			});

		const btnRow = el.createDiv({ attr: { style: 'display:flex;gap:0.5rem;margin-top:0.5rem;' } });
		this.statusEl = el.createEl('div', { attr: { style: 'font-size:0.8rem;margin-top:0.4rem;' } });

		const saveBtn = btnRow.createEl('button', {
			text: 'Speichern',
			attr: { style: 'background:var(--interactive-accent);color:#fff;border:none;border-radius:4px;padding:0.3rem 0.8rem;cursor:pointer;font-size:0.85rem;' },
		});
		saveBtn.addEventListener('click', () => this.save(slug));

		const clearBtn = btnRow.createEl('button', {
			text: 'Einschränkung entfernen',
			attr: { style: 'background:none;border:1px solid var(--background-modifier-border);border-radius:4px;padding:0.3rem 0.8rem;cursor:pointer;font-size:0.85rem;color:var(--text-muted);' },
		});
		clearBtn.addEventListener('click', () => {
			if (this.groupInput) this.groupInput.value = '';
			this.save(slug);
		});
	}

	private async save(slug: string) {
		if (!this.statusEl) return;
		const raw = this.groupInput?.value ?? '';
		const groups = raw.split(',').map(s => s.trim().toLowerCase()).filter(s => s.length > 0);
		this.statusEl.textContent = '⏳ Speichere…';
		this.statusEl.style.color = 'var(--text-muted)';
		try {
			await this.api.adminSetNoteMeta(slug, groups);
			// Update local cache
			if (groups.length > 0) {
				this.allMeta[slug] = { groups };
			} else {
				delete this.allMeta[slug];
			}
			this.statusEl.style.color = 'var(--color-green,#4caf50)';
			this.statusEl.textContent = '✓ Gespeichert';
			new Notice(`Metadaten für "${slug}" gespeichert.`);
			// Refresh list to show/hide lock icon
			const listEl = this.contentEl.querySelector('[style*="max-height:200px"]') as HTMLElement;
			if (listEl) this.refreshList(listEl);
		} catch (e) {
			this.statusEl.style.color = 'var(--color-red,#e74c3c)';
			this.statusEl.textContent = '❌ Fehler: ' + (e as Error).message;
		}
	}

	onClose() {
		this.contentEl.empty();
	}
}
