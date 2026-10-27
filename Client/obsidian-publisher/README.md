# Obsidian Publisher – Plugin

Ein Obsidian-Community-Plugin zum Synchronisieren und Verwalten eines selbst gehosteten [Obsidian Publisher Servers](../../Server/README.md).

---

## Features

- **Sync-Engine** – Checksummen-basierter Abgleich (SHA-256) zwischen lokalem Vault und Server
- **Ausstehende Änderungen** – Panel zeigt neue, geänderte und gelöschte Dateien vor dem Upload
- **Konflikt-Auflösung** – bei Konflikten wählen: lokale oder Server-Version behalten
- **Veröffentlichung umschalten** – Frontmatter-Feld per Befehl auf `true`/`false` umschalten
- **Webseite öffnen** – Server direkt aus Obsidian öffnen, bereits eingeloggt (Auto-Login)
- **Surfing-Plugin-Support** – Webseite öffnet sich im internen Browser, wenn das Surfing-Plugin aktiv ist
- **Admin-Panel** – Nutzerverwaltung, Whitelist/Blacklist und Server-Einstellungen direkt aus Obsidian
- **Protokoll-Panel** – Vollständiger Sync-Verlauf mit Zeitstempel und Statistiken
- **Auto-Sync** – optionaler automatischer Abgleich in konfigurierbarem Intervall

---

## Installation

Das Plugin ist nicht im Community-Plugin-Store verfügbar und muss manuell installiert werden.

### Manuell (empfohlen für Entwicklung)

```bash
# Plugin bauen
cd Client/obsidian-publisher
npm install
npm run build
```

Anschließend den Ordner `Client/obsidian-publisher` (oder die gebauten Dateien `main.js`, `manifest.json`, `styles.css`) in den Vault-Plugin-Ordner kopieren:

```
<vault>/.obsidian/plugins/obsidian-publisher/
├── main.js
├── manifest.json
└── styles.css    (falls vorhanden)
```

Dann in Obsidian: **Einstellungen → Community-Plugins → Obsidian Publisher** aktivieren.

---

## Konfiguration

**Einstellungen → Community-Plugins → Obsidian Publisher → Zahnrad-Icon**

| Einstellung               | Beschreibung                                              |
|---------------------------|-----------------------------------------------------------|
| **Server-URL**            | URL des Servers (z. B. `http://localhost:3000`)           |
| **Benutzername**          | Obsidian-Publisher-Benutzername                          |
| **Passwort**              | Wird nur für die erste Anmeldung verwendet                |
| **Gerätename**            | Anzeigename dieses Geräts im Admin-Panel                  |
| **Auto-Sync**             | Automatischer Abgleich aktivieren/deaktivieren           |
| **Auto-Sync-Intervall**   | Intervall in Minuten (Standard: 30)                       |
| **Sync beim Start**       | Direkt beim Öffnen von Obsidian synchronisieren          |

---

## Ribbon-Icons

| Icon             | Funktion                                                      |
|------------------|---------------------------------------------------------------|
| 🌐 Globus        | Webseite öffnen (auto-eingeloggt)                            |
| ⇄ Git-Compare    | Ausstehende Änderungen anzeigen                              |
| 🕐 History       | Protokoll-Panel öffnen                                       |
| ⚙ Settings       | Admin-Panel öffnen                                           |

---

## Befehle

Alle Befehle sind über die Befehlspalette (`Ctrl+P` / `Cmd+P`) erreichbar:

| Befehl                             | Beschreibung                                              |
|------------------------------------|-----------------------------------------------------------|
| **Ausstehende Änderungen anzeigen**| Öffnet das Änderungs-Panel                               |
| **Protokoll-Panel öffnen**         | Öffnet den Sync-Verlauf als Side-Panel                   |
| **Protokoll-Modal öffnen**         | Zeigt den Sync-Verlauf als Modal                         |
| **Admin-Panel öffnen**             | Öffnet das Admin-Modal                                   |
| **Webseite öffnen (eingeloggt)**   | Öffnet den Server im Browser, bereits angemeldet         |
| **Stille Synchronisierung**        | Synchronisiert sofort ohne Modal                         |
| **Veröffentlichung umschalten**    | Frontmatter-Publish-Feld der aktuellen Note umschalten   |

---

## Ausstehende Änderungen

Das **Änderungs-Panel** (Ribbon: ⇄) zeigt alle Unterschiede zwischen lokalem Vault und Server:

- **Hochladen** – lokale Datei ist neu oder geändert
- **Herunterladen** – Server-Datei ist neuer
- **Lokal löschen** – Datei existiert nur noch auf dem Server
- **Remote löschen** – Datei existiert nur noch lokal

Einzelne Dateien können per Checkbox ab-/ausgewählt werden. Bei Konflikten kann per Klick entschieden werden, welche Version behalten wird. Der **Sync**-Button führt die ausgewählten Aktionen aus.

---

## Veröffentlichung umschalten

Der Befehl **„Veröffentlichung umschalten"** liest das konfigurierte Publish-Feld vom Server (Admin-Panel → Einstellungen → Publish-Feld) und setzt es im Frontmatter der aktuell geöffneten Note:

- Hat die Note noch kein Frontmatter, wird es automatisch angelegt.
- `true` wird zu `false` und umgekehrt.
- Hat die Note das Feld noch nicht, wird `true` eingetragen.

```yaml
---
published: true   # ← wird zu false beim nächsten Ausführen
---
```

---

## Webseite öffnen (Auto-Login)

Der Befehl **„Webseite öffnen"** und der Globus-Ribbon öffnen den Server, ohne erneut einloggen zu müssen:

- Das Plugin generiert einen temporären Auto-Login-Link (`/auto-login?token=…`).
- Ist das [Surfing-Plugin](https://github.com/PKM-er/Obsidian-Surfing) installiert und aktiv, öffnet sich die Seite im **internen Browser**-Tab von Obsidian.
- Andernfalls wird der System-Browser geöffnet.

---

## Admin-Panel

Das Admin-Modal (Ribbon: ⚙) hat vier Tabs:

### Benutzer

Zeigt alle registrierten Benutzer. Für Nicht-Admin-Nutzer können Berechtigungen direkt bearbeitet werden. Der Admin-Eintrag ist schreibgeschützt.

| Berechtigung             | Beschreibung                                    |
|--------------------------|-------------------------------------------------|
| Hochladen                | Darf Dateien über die API hochladen             |
| Benutzer verwalten       | Kann Einladungslinks erstellen, Rechte ändern   |
| Listen verwalten         | Kann Whitelist/Blacklist bearbeiten             |
| Ordner-Einschränkung     | Schränkt den Zugriff auf bestimmte Ordner ein   |

### Einladung

Erstellt einen einmaligen Einladungslink (48h gültig) für neue Benutzer.

### White-/Blacklist

- **Whitelist**: Wenn nicht leer, werden nur die eingetragenen Slugs auf dem Server angezeigt.
- **Blacklist**: Diese Slugs werden immer versteckt.

### Einstellungen

Konfiguriert das **Publish-Feld** – den Frontmatter-Schlüssel, der als Veröffentlichungsschalter gilt. Leer lassen, um alle Notes für alle Nutzer anzuzeigen.

---

## Protokoll

Das **Protokoll-Panel** (Ribbon: 🕐) zeigt den Verlauf aller Sync-Vorgänge:

- Zeitstempel
- Anzahl hochgeladener / heruntergeladener / gelöschter Dateien
- Fehler

---

## Entwicklung

```bash
cd Client/obsidian-publisher

# Abhängigkeiten installieren
npm install

# Entwicklungs-Build (watch mode)
npm run dev

# Produktions-Build
npm run build
```

Der Build erzeugt `main.js` im Plugin-Verzeichnis. Für schnelles Testen kann `main.js` direkt in den Vault-Plugin-Ordner kopiert werden und das Plugin in Obsidian neu geladen werden (**Ctrl+R** oder Plugin deaktivieren/aktivieren).

### Technologie-Stack

| Komponente     | Technologie                                       |
|----------------|---------------------------------------------------|
| Sprache        | TypeScript                                        |
| Framework      | [Obsidian Plugin API](https://github.com/obsidianmd/obsidian-api) |
| Build          | [esbuild](https://esbuild.github.io/) 0.21        |
| API-Transport  | `fetch` (Bearer-Token-Auth)                       |
