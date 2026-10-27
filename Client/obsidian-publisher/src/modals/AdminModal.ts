import { App, Modal, Notice } from 'obsidian';
import type { ApiClient, UserWithPermissions } from '../api';

type Tab = 'users' | 'invite' | 'lists' | 'settings';

export class AdminModal extends Modal {
	private api: ApiClient;
	private activeTab: Tab = 'users';

	constructor(app: App, api: ApiClient) {
		super(app);
		this.api = api;
		this.modalEl.addClass('publisher-admin-modal');
	}

	onOpen() {
		this.render();
	}

	private render() {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.createEl('h2', { text: 'Admin-Panel' });

		// ── Tabs ────────────────────────────────────────────────────────────────
		const tabBar = contentEl.createEl('div', { cls: 'publisher-tabs' });
		const body = contentEl.createEl('div', { cls: 'publisher-tab-body' });

		const tabs: { id: Tab; label: string }[] = [
			{ id: 'users', label: 'Benutzer' },
			{ id: 'invite', label: 'Einladung' },
			{ id: 'lists', label: 'White-/Blacklist' },
			{ id: 'settings', label: 'Einstellungen' },
		];

		for (const t of tabs) {
			const btn = tabBar.createEl('button', {
				text: t.label,
				cls: 'publisher-tab' + (this.activeTab === t.id ? ' publisher-tab-active' : ''),
			});
			btn.onclick = () => {
				this.activeTab = t.id;
				this.render();
			};
		}

		// ── Tab body ────────────────────────────────────────────────────────────
		switch (this.activeTab) {
			case 'users':    this.renderUsers(body); break;
			case 'invite':   this.renderInvite(body); break;
			case 'lists':    this.renderLists(body); break;
			case 'settings': this.renderSettings(body); break;
		}
	}

	// ── Users tab ──────────────────────────────────────────────────────────────

	private renderUsers(parent: HTMLElement) {
		const loading = parent.createEl('p', { text: 'Lade Benutzer …', cls: 'publisher-progress' });

		this.api.adminListUsers().then(users => {
			loading.remove();
			if (users.length === 0) {
				parent.createEl('p', { text: 'Keine Benutzer gefunden.' });
				return;
			}
			for (const user of users) {
				this.renderUserCard(parent, user);
			}
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	private renderUserCard(parent: HTMLElement, user: UserWithPermissions) {
		const card = parent.createEl('div', { cls: 'publisher-user-card' });

		const header = card.createEl('div', { cls: 'publisher-user-header' });
		header.createEl('strong', { text: user.username });
		if (user.is_admin) {
			header.createEl('span', { text: 'Admin', cls: 'publisher-badge' });
		}

		const permsMap: Array<{ key: keyof UserWithPermissions['permissions']; label: string }> = [
			{ key: 'can_upload', label: 'Hochladen' },
			{ key: 'can_manage_users', label: 'Benutzer verwalten' },
			{ key: 'can_manage_lists', label: 'Listen verwalten' },
			{ key: 'folder_restriction', label: 'Ordner-Einschränkung' },
		];

		if (!user.is_admin) {
			const permsEl = card.createEl('div', { cls: 'publisher-perms' });
			const state = { ...user.permissions };

			for (const { key, label } of permsMap) {
				const row = permsEl.createEl('label', { cls: 'publisher-perm-row' });
				const cb = row.createEl('input');
				cb.type = 'checkbox';
				cb.checked = state[key];
				cb.onchange = () => { (state as any)[key] = cb.checked; };
				row.appendText(' ' + label);
			}

			const saveBtn = card.createEl('button', { text: 'Speichern', cls: 'mod-cta' });
			saveBtn.style.cssText = 'margin-top:8px;font-size:12px;padding:3px 10px;';
			saveBtn.onclick = async () => {
				saveBtn.setText('…').disabled = true;
				try {
					await this.api.adminUpdatePermissions(user.id, state);
					new Notice(`Berechtigungen für ${user.username} gespeichert`);
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern').disabled = false; }, 3000);
			};
		}

		if (!user.is_admin) {
			const delBtn = card.createEl('button', {
				text: 'Benutzer löschen',
				cls: 'publisher-danger-btn',
			});
			delBtn.onclick = async () => {
				if (!confirm(`Benutzer „${user.username}" wirklich löschen?`)) return;
				try {
					await this.api.adminDeleteUser(user.id);
					new Notice(`${user.username} gelöscht`);
					card.remove();
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
				}
			};
		}
	}

	// ── Invite tab ─────────────────────────────────────────────────────────────

	private renderInvite(parent: HTMLElement) {
		parent.createEl('p', {
			text: 'Erstelle einen einmaligen Einladungslink für einen neuen Benutzer.',
		});

		const btn = parent.createEl('button', { text: 'Einladungslink erstellen', cls: 'mod-cta' });
		const result = parent.createEl('div', { cls: 'publisher-invite-result' });
		result.style.display = 'none';

		btn.onclick = async () => {
			btn.setText('…').disabled = true;
			result.style.display = 'none';
			try {
				const { url, expires_hours } = await this.api.adminCreateInvite();
				result.style.display = 'block';
				result.empty();
				result.createEl('p', {
					text: `Gültig für ${expires_hours} Stunde(n). Nur einmal verwendbar.`,
					cls: 'publisher-progress',
				});
				const input = result.createEl('input', { cls: 'publisher-invite-url' });
				input.value = url;
				input.readOnly = true;
				const copyBtn = result.createEl('button', { text: 'Kopieren' });
				copyBtn.onclick = () => {
					navigator.clipboard.writeText(url);
					copyBtn.setText('✓ Kopiert');
					setTimeout(() => copyBtn.setText('Kopieren'), 2000);
				};
			} catch (e) {
				new Notice('Fehler: ' + (e as Error).message);
			}
			btn.setText('Einladungslink erstellen').disabled = false;
		};
	}

	// ── Lists tab ──────────────────────────────────────────────────────────────

	private renderLists(parent: HTMLElement) {
		const loading = parent.createEl('p', { text: 'Lade Listen …', cls: 'publisher-progress' });

		this.api.adminGetLists().then(({ whitelist, blacklist }) => {
			loading.remove();

			parent.createEl('p', {
				text: 'Eine Whitelist bedeutet: nur diese Slugs werden veröffentlicht. Leer = alle erlaubt.',
			}).style.fontSize = '13px';

			parent.createEl('label', { text: 'Whitelist (ein Slug pro Zeile):' });
			const wlArea = parent.createEl('textarea', { cls: 'publisher-list-textarea' });
			wlArea.rows = 6;
			wlArea.value = whitelist.join('\n');

			parent.createEl('label', { text: 'Blacklist (ein Slug pro Zeile):' });
			const blArea = parent.createEl('textarea', { cls: 'publisher-list-textarea' });
			blArea.rows = 6;
			blArea.value = blacklist.join('\n');

			const saveBtn = parent.createEl('button', { text: 'Speichern', cls: 'mod-cta' });
			saveBtn.onclick = async () => {
				const wl = wlArea.value.split('\n').map(s => s.trim()).filter(Boolean);
				const bl = blArea.value.split('\n').map(s => s.trim()).filter(Boolean);
				saveBtn.setText('…').disabled = true;
				try {
					await this.api.adminSetLists(wl, bl);
					new Notice('Listen gespeichert');
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern').disabled = false; }, 3000);
			};
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	// ── Settings tab ───────────────────────────────────────────────────────────

	private renderSettings(parent: HTMLElement) {
		const loading = parent.createEl('p', { text: 'Lade Einstellungen …', cls: 'publisher-progress' });

		this.api.adminGetSettings().then(({ publish_field }) => {
			loading.remove();

			parent.createEl('p', {
				text: 'Lege fest, welches Frontmatter-Feld als Veröffentlichungsschalter gilt.',
			}).style.fontSize = '13px';

			parent.createEl('label', { text: 'Publish-Feld (Frontmatter-Key):' });
			const input = parent.createEl('input');
			input.type = 'text';
			input.value = publish_field;
			input.placeholder = 'z. B. published';
			input.style.cssText = 'display:block;width:100%;margin:6px 0 4px 0;padding:5px 8px;font-size:13px;border-radius:4px;border:1px solid var(--background-modifier-border);background:var(--background-secondary);color:var(--text-normal);';

			parent.createEl('p', {
				text: 'Leer lassen, um alle Notizen anzuzeigen (kein Filter). Wenn gesetzt, werden nur Notizen mit diesem Feld = true für nicht-Admins angezeigt.',
				cls: 'publisher-progress',
			});

			const saveBtn = parent.createEl('button', { text: 'Speichern', cls: 'mod-cta' });
			saveBtn.onclick = async () => {
				saveBtn.setText('…').disabled = true;
				try {
					await this.api.adminSetSettings({ publish_field: input.value.trim() });
					new Notice('Einstellungen gespeichert');
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern').disabled = false; }, 3000);
			};
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	onClose() {
		this.contentEl.empty();
	}
}
