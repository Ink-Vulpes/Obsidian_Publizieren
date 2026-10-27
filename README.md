# Obsidian Publisher

Ein selbst gehostetes System zum Veröffentlichen von Obsidian-Vaults als Website – mit Authentifizierung, Benutzerverwaltung und direkter Sync-Integration über ein Obsidian-Plugin.

```
obsidian-publisher/
├── Server/        Rust/Axum-Webserver + MySQL
└── Client/        Obsidian-Plugin (TypeScript)
```

---

## Überblick

| Komponente | Beschreibung |
|------------|-------------|
| **Server** | Rendert den Vault als Website mit Explorer, Graph, Suche und Admin-Panel |
| **Plugin** | Synchronisiert den lokalen Vault mit dem Server direkt aus Obsidian |

Das System ersetzt Cloud-Dienste wie Obsidian Publish durch eine vollständig selbst kontrollierte Lösung.

---

## Schnellstart

### 1. Server starten

```bash
cd Server
cp config.yml.example config.yml   # Konfiguration anpassen
docker compose up -d
```

→ **http://localhost:3000**

### 2. Plugin installieren

```bash
cd Client/obsidian-publisher
npm install && npm run build
```

Den Ordner in den Vault-Plugin-Ordner kopieren:

```
<vault>/.obsidian/plugins/obsidian-publisher/
```

In Obsidian: **Einstellungen → Community-Plugins → Obsidian Publisher** aktivieren und Server-URL + Zugangsdaten eintragen.

---

## Dokumentation

- [Server →](Server/README.md) – Docker-Setup, Konfiguration, API-Referenz, Benutzer & Berechtigungen
- [Plugin →](Client/obsidian-publisher/README.md) – Installation, Befehle, Ribbon-Icons, Admin-Modal

---

## Technologie

| Schicht    | Technologie                                  |
|------------|----------------------------------------------|
| Server     | Rust · Axum · Tera · sqlx · MySQL 8          |
| Plugin     | TypeScript · Obsidian API · esbuild          |
| Deployment | Docker · Docker Compose · ghcr.io            |
