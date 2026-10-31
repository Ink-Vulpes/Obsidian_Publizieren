import { App, Modal, Notice } from 'obsidian';
import type { ApiClient, Group, UserWithPermissions } from '../api';

type Tab = 'users' | 'invite' | 'lists' | 'groups' | 'settings';

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
			{ id: 'groups', label: 'Gruppen' },
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
			case 'groups':   this.renderGroups(body); break;
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
			{ key: 'sees_published', label: 'Sieht alle veröffentlichten Notizen (ignoriert Gruppen)' },
			{ key: 'sees_all', label: 'Sieht alles inkl. unveröffentlichte Notizen' },
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
				saveBtn.setText('…'); saveBtn.disabled = true;
				try {
					await this.api.adminUpdatePermissions(user.id, state);
					new Notice(`Berechtigungen für ${user.username} gespeichert`);
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern'); saveBtn.disabled = false; }, 3000);
			};
		}

		if (!user.is_admin) {
			// ── Groups section ──────────────────────────────────────────────
			const groupsSection = card.createEl('div');
			groupsSection.style.cssText = 'margin-top:10px;padding-top:10px;border-top:1px solid var(--background-modifier-border);';
			groupsSection.createEl('p', { text: 'Gruppen:', cls: 'publisher-progress' }).style.cssText = 'font-size:11px;margin-bottom:5px;';
			const groupTagsEl = groupsSection.createEl('div');
			groupTagsEl.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin-bottom:6px;';

			const renderGroupTags = (currentGroups: string[], allGroups: Group[]) => {
				groupTagsEl.empty();
				if (currentGroups.length === 0) {
					groupTagsEl.createEl('span', { text: 'Keine Gruppen', cls: 'publisher-progress' }).style.fontSize = '11px';
				}
				for (const gName of currentGroups) {
					const tag = groupTagsEl.createEl('span');
					tag.style.cssText = 'display:inline-flex;align-items:center;gap:3px;background:var(--background-secondary);border:1px solid var(--background-modifier-border);border-radius:4px;padding:1px 6px;font-size:11px;font-family:monospace;';
					tag.appendText(gName);
					const rmBtn = tag.createEl('button', { text: '×' });
					rmBtn.style.cssText = 'background:none;border:none;color:var(--text-muted);cursor:pointer;font-size:13px;padding:0;line-height:1;';
					rmBtn.onclick = async () => {
						const g = allGroups.find(x => x.name === gName);
						if (!g) return;
						try {
							await this.api.adminRemoveGroupMember(g.id, user.id);
							const idx = currentGroups.indexOf(gName);
							if (idx !== -1) currentGroups.splice(idx, 1);
							renderGroupTags(currentGroups, allGroups);
							// refresh select
							addOpt.innerHTML = '<option value="">— Gruppe hinzufügen —</option>';
							for (const x of allGroups) {
								if (!currentGroups.includes(x.name)) addOpt.createEl('option', { value: String(x.id), text: x.name });
							}
						} catch (e) { new Notice('Fehler: ' + (e as Error).message); }
					};
				}
			};

			const addRow = groupsSection.createEl('div');
			addRow.style.cssText = 'display:flex;gap:6px;';
			const addOpt = addRow.createEl('select');
			addOpt.style.cssText = 'flex:1;padding:3px 6px;font-size:11px;border-radius:4px;border:1px solid var(--background-modifier-border);background:var(--background-secondary);color:var(--text-normal);';
			const addBtn = addRow.createEl('button', { text: '+ Add', cls: 'mod-cta' });
			addBtn.style.cssText = 'font-size:11px;padding:2px 8px;';

			// Load groups async
			const currentGroups = [...user.groups];
			this.api.adminListGroups().then(allGroups => {
				renderGroupTags(currentGroups, allGroups);
				addOpt.createEl('option', { value: '', text: '— Gruppe hinzufügen —' });
				for (const g of allGroups) {
					if (!currentGroups.includes(g.name)) addOpt.createEl('option', { value: String(g.id), text: g.name });
				}
				addBtn.onclick = async () => {
					const gid = parseInt(addOpt.value);
					if (!gid) return;
					const g = allGroups.find(x => x.id === gid);
					if (!g) return;
					addBtn.setText('…'); addBtn.disabled = true;
					try {
						await this.api.adminAddGroupMember(gid, user.id);
						currentGroups.push(g.name);
						renderGroupTags(currentGroups, allGroups);
						// remove from select
						addOpt.querySelector(`option[value="${gid}"]`)?.remove();
					} catch (e) { new Notice('Fehler: ' + (e as Error).message); }
					addBtn.setText('+ Add'); addBtn.disabled = false;
				};
			}).catch(() => { groupsSection.style.display = 'none'; });

			// ── Delete button ───────────────────────────────────────────────
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
				saveBtn.setText('…'); saveBtn.disabled = true;
				try {
					await this.api.adminSetLists(wl, bl);
					new Notice('Listen gespeichert');
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern'); saveBtn.disabled = false; }, 3000);
			};
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	// ── Settings tab ───────────────────────────────────────────────────────────

	private renderSettings(parent: HTMLElement) {
		const loading = parent.createEl('p', { text: 'Lade Einstellungen …', cls: 'publisher-progress' });

		const inputStyle = 'display:block;width:100%;margin:4px 0 12px;padding:5px 8px;font-size:13px;border-radius:4px;border:1px solid var(--background-modifier-border);background:var(--background-secondary);color:var(--text-normal);';
		const hintStyle = 'font-size:11px;color:var(--text-muted);margin:-8px 0 14px;';

		this.api.adminGetSettings().then(({ publish_field, group_field }) => {
			loading.remove();

			parent.createEl('label', { text: 'Publish-Feld (Frontmatter-Key):' });
			const publishInput = parent.createEl('input');
			publishInput.type = 'text';
			publishInput.value = publish_field;
			publishInput.placeholder = 'z. B. published';
			publishInput.style.cssText = inputStyle;
			parent.createEl('p', {
				text: 'Leer lassen, um alle Notizen anzuzeigen (kein Filter). Wenn gesetzt, werden nur Notizen mit diesem Feld = true angezeigt.',
				cls: 'publisher-progress',
			}).style.cssText = hintStyle;

			parent.createEl('label', { text: 'Gruppen-Feld (Frontmatter-Key):' });
			const groupInput = parent.createEl('input');
			groupInput.type = 'text';
			groupInput.value = group_field ?? 'group';
			groupInput.placeholder = 'z. B. group';
			groupInput.style.cssText = inputStyle;
			parent.createEl('p', {
				text: 'Der Frontmatter-Key, der den Gruppennamen enthält (Standard: "group").',
				cls: 'publisher-progress',
			}).style.cssText = hintStyle;

			const saveBtn = parent.createEl('button', { text: 'Speichern', cls: 'mod-cta' });
			saveBtn.onclick = async () => {
				saveBtn.setText('…'); saveBtn.disabled = true;
				try {
					await this.api.adminSetSettings({
						publish_field: publishInput.value.trim(),
						group_field: groupInput.value.trim() || 'group',
					});
					new Notice('Einstellungen gespeichert');
					saveBtn.setText('✓ Gespeichert');
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					saveBtn.setText('Fehler');
				}
				setTimeout(() => { saveBtn.setText('Speichern'); saveBtn.disabled = false; }, 3000);
			};
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	// ── Groups tab ─────────────────────────────────────────────────────────────

	private renderGroups(parent: HTMLElement) {
		const loading = parent.createEl('p', { text: 'Lade Gruppen …', cls: 'publisher-progress' });

		Promise.all([this.api.adminListGroups(), this.api.adminListUsers()]).then(([groups, users]) => {
			loading.remove();

			parent.createEl('p', {
				text: 'Gruppen erlauben den Zugriff auf Notizen einzuschränken. Frontmatter: group: gruppenname',
			}).style.fontSize = '13px';

			const nonAdminUsers = users.filter(u => !u.is_admin);

			for (const group of groups) {
				this.renderGroupCard(parent, group, nonAdminUsers, () => {
					parent.empty();
					this.renderGroups(parent);
				});
			}

			parent.createEl('hr');
			parent.createEl('label', { text: 'Neue Gruppe erstellen:' });
			const row = parent.createEl('div');
			row.style.cssText = 'display:flex;gap:8px;margin-top:6px;';
			const nameInput = row.createEl('input');
			nameInput.type = 'text';
			nameInput.placeholder = 'Gruppenname (z.B. premium)';
			nameInput.style.cssText = 'flex:1;padding:5px 8px;font-size:13px;border-radius:4px;border:1px solid var(--background-modifier-border);background:var(--background-secondary);color:var(--text-normal);';
			const createBtn = row.createEl('button', { text: '+ Erstellen', cls: 'mod-cta' });
			createBtn.onclick = async () => {
				const name = nameInput.value.trim();
				if (!name) return;
				createBtn.setText('…'); createBtn.disabled = true;
				try {
					await this.api.adminCreateGroup(name);
					new Notice(`Gruppe „${name}" erstellt`);
					parent.empty();
					this.renderGroups(parent);
				} catch (e) {
					new Notice('Fehler: ' + (e as Error).message);
					createBtn.setText('+ Erstellen'); createBtn.disabled = false;
				}
			};
		}).catch(e => {
			loading.setText('Fehler: ' + (e as Error).message);
			loading.style.color = 'var(--color-red)';
		});
	}

	private renderGroupCard(
		parent: HTMLElement,
		group: Group,
		allUsers: UserWithPermissions[],
		onRefresh: () => void,
	) {
		const card = parent.createEl('div', { cls: 'publisher-user-card' });

		const header = card.createEl('div', { cls: 'publisher-user-header' });
		header.createEl('strong', { text: group.name });
		header.createEl('span', { text: `ID ${group.id}`, cls: 'publisher-badge' });

		const delBtn = card.createEl('button', { text: 'Löschen', cls: 'publisher-danger-btn' });
		delBtn.onclick = async () => {
			if (!confirm(`Gruppe „${group.name}" wirklich löschen?`)) return;
			try {
				await this.api.adminDeleteGroup(group.id);
				new Notice(`Gruppe „${group.name}" gelöscht`);
				onRefresh();
			} catch (e) {
				new Notice('Fehler: ' + (e as Error).message);
			}
		};

		const membersLabel = card.createEl('p', { text: 'Mitglieder:' });
		membersLabel.style.cssText = 'font-size:12px;color:var(--text-muted);margin:6px 0 3px;';

		const membersList = card.createEl('div');
		membersList.style.cssText = 'display:flex;flex-wrap:wrap;gap:5px;margin-bottom:8px;';

		if (group.members.length === 0) {
			membersList.createEl('span', { text: 'Keine Mitglieder', cls: 'publisher-progress' });
		} else {
			for (const m of group.members) {
				const tag = membersList.createEl('span');
				tag.style.cssText = 'display:inline-flex;align-items:center;gap:4px;background:var(--background-secondary);border:1px solid var(--background-modifier-border);border-radius:4px;padding:1px 6px;font-size:12px;';
				tag.appendText(m.username);
				const rmBtn = tag.createEl('button', { text: '×' });
				rmBtn.style.cssText = 'background:none;border:none;color:var(--text-muted);cursor:pointer;font-size:14px;padding:0;line-height:1;';
				rmBtn.onclick = async () => {
					try {
						await this.api.adminRemoveGroupMember(group.id, m.id);
						new Notice(`${m.username} entfernt`);
						onRefresh();
					} catch (e) {
						new Notice('Fehler: ' + (e as Error).message);
					}
				};
			}
		}

		const addRow = card.createEl('div');
		addRow.style.cssText = 'display:flex;gap:6px;';
		const sel = addRow.createEl('select');
		sel.style.cssText = 'flex:1;padding:3px 6px;font-size:12px;border-radius:4px;border:1px solid var(--background-modifier-border);background:var(--background-secondary);color:var(--text-normal);';
		sel.createEl('option', { value: '', text: '— Nutzer hinzufügen —' });
		for (const u of allUsers) {
			if (!group.members.find(m => m.id === u.id)) {
				sel.createEl('option', { value: String(u.id), text: u.username });
			}
		}
		const addBtn = addRow.createEl('button', { text: '+ Add', cls: 'mod-cta' });
		addBtn.style.cssText = 'font-size:12px;padding:3px 10px;';
		addBtn.onclick = async () => {
			const uid = parseInt(sel.value);
			if (!uid) return;
			addBtn.setText('…'); addBtn.disabled = true;
			try {
				await this.api.adminAddGroupMember(group.id, uid);
				new Notice('Mitglied hinzugefügt');
				onRefresh();
			} catch (e) {
				new Notice('Fehler: ' + (e as Error).message);
				addBtn.setText('+ Add'); addBtn.disabled = false;
			}
		};
	}

	onClose() {
		this.contentEl.empty();
	}
}
