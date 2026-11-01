# Obsidian Publisher — Plugin

An Obsidian community plugin for syncing and managing a self-hosted [Obsidian Publisher server](../../Server/README.md).

---

## Features

- **Sync engine** — checksum-based diff (SHA-256) between local vault and server
- **Pending changes panel** — shows new, modified, and deleted files before upload
- **Conflict resolution** — choose per-file whether to keep local or server version
- **Toggle publish** — flip the frontmatter publish field on/off with a command
- **Open website** — open the server directly from Obsidian, already logged in (auto-login)
- **Surfing plugin support** — website opens in the internal browser when Surfing is active
- **Admin panel** — manage users, groups, whitelist/blacklist, and server settings from Obsidian
- **Log panel** — full sync history with timestamps and statistics
- **Auto-sync** — optional automatic sync at a configurable interval
- **External metadata** — override group assignments for notes without touching the vault files

---

## Installation

The plugin is not available in the Community Plugin store and must be installed manually.

### From a release (recommended)

1. Download `main.js` and `manifest.json` from the [latest release](https://github.com/Ink-Vulpes/Obsidian_Publizieren/releases).
2. Place them in your vault's plugin folder:

```
<vault>/.obsidian/plugins/obsidian-publisher/
├── main.js
└── manifest.json
```

3. In Obsidian: **Settings → Community Plugins → Obsidian Publisher** — enable.

### Build from source

```bash
cd Client/obsidian-publisher
npm install
npm run build
```

Copy the built `main.js` and `manifest.json` into the vault plugin folder above.

---

## Configuration

**Settings → Community Plugins → Obsidian Publisher → gear icon**

| Setting                  | Description                                               |
|--------------------------|-----------------------------------------------------------|
| **Server URL**           | URL of the server (e.g. `http://localhost:3000`)          |
| **Username**             | Obsidian Publisher username                               |
| **Password**             | Used only for initial login                               |
| **Device name**          | Display name for this device in the admin panel           |
| **Auto-sync**            | Enable/disable automatic syncing                          |
| **Auto-sync interval**   | Interval in minutes (default: 30)                         |
| **Sync on startup**      | Sync immediately when Obsidian opens                      |

---

## Ribbon icons

| Icon           | Function                                              |
|----------------|-------------------------------------------------------|
| 🌐 Globe       | Open website (auto-logged in)                         |
| ⇄ Git-compare  | Show pending changes                                  |
| 🕐 History     | Open log panel                                        |
| ⚙ Settings     | Open admin panel                                      |

---

## Commands

All commands are accessible via the command palette (`Ctrl+P` / `Cmd+P`):

| Command                        | Description                                               |
|--------------------------------|-----------------------------------------------------------|
| **Show pending changes**       | Opens the changes panel                                   |
| **Open log panel**             | Opens the sync history as a side panel                    |
| **Open log modal**             | Shows the sync history as a modal                         |
| **Open admin panel**           | Opens the admin modal                                     |
| **Open website (logged in)**   | Opens the server in the browser, already authenticated    |
| **Silent sync**                | Syncs immediately without showing a modal                 |
| **Toggle publish**             | Flips the publish frontmatter field of the current note   |
| **Reset server and upload vault** | Wipes server files and re-uploads the entire vault     |

---

## Pending changes

The **changes panel** (ribbon: ⇄) shows all differences between the local vault and the server:

- **Upload** — local file is new or modified
- **Download** — server file is newer
- **Delete locally** — file only exists on the server
- **Delete remotely** — file only exists locally

Individual files can be checked/unchecked. Conflicts can be resolved per-file by choosing which version to keep. The **Sync** button executes the selected actions.

---

## Toggle publish

The **Toggle publish** command reads the configured publish field from the server (Admin Panel → Settings → Publish field) and sets it in the frontmatter of the currently open note:

- If the note has no frontmatter yet, it is created automatically.
- `true` becomes `false` and vice versa.
- If the note does not yet have the field, it is set to `true`.

```yaml
---
published: true   # ← becomes false on next run
---
```

---

## Open website (auto-login)

The **Open website** command and the globe ribbon open the server without requiring a second login:

- The plugin generates a temporary auto-login link (`/auto-login?token=…`).
- If the [Surfing plugin](https://github.com/PKM-er/Obsidian-Surfing) is installed and active, the page opens in Obsidian's **internal browser tab**.
- Otherwise, the system browser is opened.

---

## Admin panel

The admin modal (ribbon: ⚙) has the following tabs:

### Users

Shows all registered users. Permissions can be edited for non-admin users. The admin entry is read-only.

| Permission               | Description                                       |
|--------------------------|---------------------------------------------------|
| Upload                   | May upload files via the API                      |
| Manage users             | Can create invitation links, edit permissions     |
| Manage lists             | Can edit whitelist/blacklist                      |
| Folder restriction       | Restricts access to specific folders              |
| Sees published           | Sees all published notes, ignoring group access   |
| Sees all                 | Sees everything including unpublished notes        |

### Invitation

Creates a one-time invitation link (valid for 48 hours) for new users.

### Whitelist / Blacklist

- **Whitelist**: If non-empty, only the listed slugs are shown on the server.
- **Blacklist**: These slugs are always hidden.

### Settings

Configures the **publish field** — the frontmatter key used as a publish toggle. Also configures the **group field**. Leave the publish field empty to show all notes to all users.

### External Metadata

Override the group assignment for notes without touching the vault files. Useful when the vault is read-only or the notes are shared with others.

---

## Log

The **log panel** (ribbon: 🕐) shows the history of all sync operations:

- Timestamp
- Number of uploaded / downloaded / deleted files
- Errors

---

## Development

```bash
cd Client/obsidian-publisher

# Install dependencies
npm install

# Development build (watch mode)
npm run dev

# Production build
npm run build
```

The build produces `main.js` in the plugin directory. For quick testing, copy `main.js` directly to the vault plugin folder and reload the plugin in Obsidian (**Ctrl+R** or disable/enable the plugin).

---

## Tech stack

| Component   | Technology                                                        |
|-------------|-------------------------------------------------------------------|
| Language    | TypeScript                                                        |
| Framework   | [Obsidian Plugin API](https://github.com/obsidianmd/obsidian-api) |
| Build       | [esbuild](https://esbuild.github.io/) 0.21                        |
| API         | `fetch` (Bearer token auth, JWT)                                  |
