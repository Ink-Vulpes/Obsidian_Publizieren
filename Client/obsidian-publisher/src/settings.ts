import { App, PluginSettingTab, Setting } from 'obsidian';
import type ObsidianPublisher from './main';

export interface PublisherSettings {
	serverUrl: string;
	username: string;
	password: string;
	autoSync: boolean;
	autoSyncIntervalMinutes: number;
	deviceName: string;
	syncOnStartup: boolean;
}

export const DEFAULT_SETTINGS: PublisherSettings = {
	serverUrl: 'https://localhost:3000',
	username: '',
	password: '',
	autoSync: false,
	autoSyncIntervalMinutes: 30,
	deviceName: '',
	syncOnStartup: false,
};

export class PublisherSettingTab extends PluginSettingTab {
	plugin: ObsidianPublisher;

	constructor(app: App, plugin: ObsidianPublisher) {
		super(app, plugin);
		this.plugin = plugin;
	}

	display(): void {
		const { containerEl } = this;
		containerEl.empty();
		containerEl.createEl('h2', { text: 'Publizieren' });

		// ── Server ──────────────────────────────────────────────────────────────
		containerEl.createEl('h3', { text: 'Server' });

		new Setting(containerEl)
			.setName('Server-URL')
			.setDesc('URL deines Publisher-Servers (z. B. https://notes.example.com)')
			.addText(text => text
				.setPlaceholder('http://localhost:3000')
				.setValue(this.plugin.settings.serverUrl)
				.onChange(async value => {
					this.plugin.settings.serverUrl = value.trim();
					await this.plugin.saveSettings();
				}));

		new Setting(containerEl)
			.setName('Benutzername')
			.addText(text => text
				.setValue(this.plugin.settings.username)
				.onChange(async value => {
					this.plugin.settings.username = value.trim();
					await this.plugin.saveSettings();
				}));

		new Setting(containerEl)
			.setName('Passwort')
			.addText(text => {
				text.inputEl.type = 'password';
				text.setValue(this.plugin.settings.password)
					.onChange(async value => {
						this.plugin.settings.password = value;
						await this.plugin.saveSettings();
					});
			});

		new Setting(containerEl)
			.setName('Gerätename')
			.setDesc('Identifiziert dieses Gerät in der Sync-Übersicht des Servers')
			.addText(text => text
				.setPlaceholder('Mein PC')
				.setValue(this.plugin.settings.deviceName)
				.onChange(async value => {
					this.plugin.settings.deviceName = value.trim();
					await this.plugin.saveSettings();
				}));

		// ── Synchronisierung ─────────────────────────────────────────────────────
		containerEl.createEl('h3', { text: 'Synchronisierung' });

		new Setting(containerEl)
			.setName('Auto-Sync aktivieren')
			.setDesc('Vault automatisch in regelmäßigen Abständen synchronisieren')
			.addToggle(toggle => toggle
				.setValue(this.plugin.settings.autoSync)
				.onChange(async value => {
					this.plugin.settings.autoSync = value;
					await this.plugin.saveSettings();
					if (value) this.plugin.startAutoSync();
					else this.plugin.stopAutoSync();
					this.display();
				}));

		if (this.plugin.settings.autoSync) {
			new Setting(containerEl)
				.setName('Auto-Sync-Intervall')
				.setDesc(`Aktuell: alle ${this.plugin.settings.autoSyncIntervalMinutes} Minuten`)
				.addSlider(slider => slider
					.setLimits(5, 120, 5)
					.setValue(this.plugin.settings.autoSyncIntervalMinutes)
					.setDynamicTooltip()
					.onChange(async value => {
						this.plugin.settings.autoSyncIntervalMinutes = value;
						await this.plugin.saveSettings();
						this.plugin.startAutoSync();
					}));
		}

		new Setting(containerEl)
			.setName('Beim Start synchronisieren')
			.setDesc('Direkt nach dem Öffnen von Obsidian synchronisieren')
			.addToggle(toggle => toggle
				.setValue(this.plugin.settings.syncOnStartup)
				.onChange(async value => {
					this.plugin.settings.syncOnStartup = value;
					await this.plugin.saveSettings();
				}));

		// ── Verbindungstest ──────────────────────────────────────────────────────
		containerEl.createEl('h3', { text: 'Verbindung' });

		new Setting(containerEl)
			.setName('Verbindung testen')
			.setDesc('Prüft ob Server erreichbar ist und die Zugangsdaten stimmen')
			.addButton(btn => btn
				.setButtonText('Testen')
				.onClick(async () => {
					btn.setButtonText('…').setDisabled(true);
					try {
						await this.plugin.api.login(
							this.plugin.settings.username,
							this.plugin.settings.password,
						);
						btn.setButtonText('✓ Verbunden');
						btn.buttonEl.style.color = 'var(--color-green)';
					} catch (e) {
						btn.setButtonText('✗ ' + (e as Error).message);
						btn.buttonEl.style.color = 'var(--color-red)';
					}
					setTimeout(() => {
						btn.setButtonText('Testen').setDisabled(false);
						btn.buttonEl.style.color = '';
					}, 4000);
				}));
	}
}
