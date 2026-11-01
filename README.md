# Obsidian Publisher

A self-hosted system for publishing Obsidian vaults as a website — with authentication, user management, groups, and direct sync integration via an Obsidian plugin.

```
obsidian-publisher/
├── Server/        Rust/Axum web server + MySQL
└── Client/        Obsidian plugin (TypeScript)
```

---

## Overview

| Component | Description |
|-----------|-------------|
| **Server** | Renders the vault as a website with explorer, graph, search, and admin panel |
| **Plugin** | Syncs the local vault with the server directly from Obsidian |

This system replaces cloud services like Obsidian Publish with a fully self-controlled solution.

---

## Quick start

### 1. Start the server

```bash
cd Server
cp config.yml.example config.yml   # adjust configuration
docker compose up -d
```

→ **http://localhost:3000**

### 2. Install the plugin

Download the latest release from [GitHub Releases](https://github.com/Ink-Vulpes/Obsidian_Publizieren/releases) and place the files in your vault's plugin folder:

```
<vault>/.obsidian/plugins/obsidian-publisher/
├── main.js
└── manifest.json
```

Then in Obsidian: **Settings → Community Plugins → Obsidian Publisher** — enable and enter your server URL and credentials.

---

## Documentation

- [Server →](Server/README.md) — Docker setup, configuration, API reference, users & permissions
- [Plugin →](Client/obsidian-publisher/README.md) — installation, commands, ribbon icons, admin modal

---

## Tech stack

| Layer      | Technology                                   |
|------------|----------------------------------------------|
| Server     | Rust · Axum · Tera · sqlx · MySQL 8          |
| Plugin     | TypeScript · Obsidian API · esbuild          |
| Deployment | Docker · Docker Compose · ghcr.io            |
