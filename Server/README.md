# Obsidian Publisher – Server

Ein selbst gehosteter Web-Server, der eine Obsidian-Vault live im Browser rendert – ähnlich wie [Quartz](https://quartz.jzhao.xyz/), aber vollständig in Rust mit MySQL-Authentifizierung und HTTP-Upload.

![Rust](https://img.shields.io/badge/built_with-Rust-orange) ![Docker](https://img.shields.io/badge/deploy-Docker-blue)

---

## Features

- **3-Spalten-Layout** (Explorer · Inhalt · TOC/Backlinks/Graph) mit Dark/Light-Mode
- **Obsidian-Wikilinks** `[[Note]]` und `[[Note|Alias]]` werden zu klickbaren Links aufgelöst
- **Backlinks** – welche Notes verweisen auf die aktuelle?
- **Lokaler D3-Graph** – Beziehungen zwischen Notes visualisiert
- **Volltextsuche** (Ctrl+K) über alle veröffentlichten Notes
- **LaTeX / MathJax 3** – `$inline$` und `$$block$$` werden gerendert
- **Bild-Embeds** `![[bild.png]]` als `<img>`, Pfad wird vault-weit aufgelöst
- **PDF-Links** öffnen die Datei direkt im Browser
- **Callouts** (`> [!info]`, `> [!warning]` usw.) im Obsidian-Stil
- **Canvas** (`.canvas`) und **Excalidraw** werden gerendert
- **Tags**, **Inhaltsverzeichnis**, **Anker-Links** an Überschriften
- **Authentifizierung** – Login, Einladungslinks, Rollen und Berechtigungen per Admin-Panel
- **Publish-Feld-Filter** – nur Notes mit einem konfigurierbaren Frontmatter-Feld `= true` werden für Nicht-Admins angezeigt
- **HTTP-Upload** via Obsidian-Plugin oder `publish.py` – Checksummen-basierter Sync (SHA-256)
- **Auto-Login** – Einmaliger Token-Link für nahtlose Weiterleitung aus dem Obsidian-Plugin

---

## Schnellstart mit Docker

### Voraussetzungen

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (Windows/Mac/Linux)
- Python 3.9+ (nur für `publish.py`)

### 1. Repository klonen

```bash
git clone <repo-url>
cd Server
```

### 2. Konfiguration anlegen

```bash
cp config.yml.example config.yml
```

Wichtige Einstellungen in `config.yml`:

```yaml
server:
  port: 3000
  vault_path: "/vault"
  site_title: "Mein Digital Garden"
  base_url: "http://localhost:3000"

database:
  url: "mysql://obsidian:changeme@mysql:3306/obsidian"

auth:
  session_secret: "MINDESTENS_64_ZUFAELLIGE_ZEICHEN_HIER_EINTRAGEN!!!"
  session_duration_hours: 168

admin:
  username: "admin"
  password: "changeme"   # beim ersten Start ändern!
```

Optionale Anpassungen per `.env`-Datei:

```env
WEB_PORT=3000
MYSQL_ROOT_PASSWORD=sicheres-root-passwort
MYSQL_PASS=sicheres-db-passwort
RUST_LOG=info
```

### 3. Container starten

```bash
docker compose up -d
```

Der Web-Server läuft auf **http://localhost:3000**.

Beim ersten Start:
1. Datenbank-Migrationen werden automatisch ausgeführt.
2. Der Admin-Benutzer (aus `config.yml`) wird angelegt, sofern noch kein Benutzer existiert.
3. Der Vault wird indiziert.

### 4. Admin-Panel

Unter **http://localhost:3000/admin** (nach Login):

| Tab | Funktion |
|-----|----------|
| **Benutzer** | Nutzer anzeigen, Berechtigungen bearbeiten, Einladungslinks erstellen |
| **Whitelist / Blacklist** | Sichtbarkeit einzelner Notes steuern |
| **Einstellungen** | Publish-Feld konfigurieren |

### 5. Publish-Feld konfigurieren

Im Admin-Panel → **Einstellungen** kann ein Frontmatter-Feld als Veröffentlichungsschalter festgelegt werden (Standard: `published`).

Notes mit `published: true` (bzw. dem konfigurierten Feldnamen) sind für alle Nutzer sichtbar. Notes ohne dieses Feld oder mit `false` sind nur für Admins sichtbar. Wird das Feld leer gelassen, sind alle Notes für alle Nutzer sichtbar.

Beispiel-Frontmatter:

```yaml
---
published: true
title: Mein Artikel
---
```

---

## Image veröffentlichen (ghcr.io)

Das fertige Docker-Image liegt auf der **GitHub Container Registry** unter `ghcr.io/ink-vulpes/obsidian-publisher`.

### Image bauen & pushen

```bash
bash push-image.sh          # baut und pusht als :latest
bash push-image.sh 1.0.0    # baut und pusht als :1.0.0 + :latest
```

### Auf dem Server deployen

```bash
echo "DEIN_TOKEN" | docker login ghcr.io -u Ink-Vulpes --password-stdin
docker compose pull
docker compose up -d
```

Update:

```bash
docker compose pull && docker compose up -d
```

---

## Notes hochladen

### Via Obsidian-Plugin (empfohlen)

Das Plugin synchronisiert den Vault direkt über die HTTP-API. Siehe [Client README](../Client/obsidian-publisher/README.md).

### Via publish.py

```bash
# Lokal
python publish.py /pfad/zur/obsidian-vault

# Remote
python publish.py /pfad/zur/obsidian-vault --server https://notes.example.com --user admin --yes
```

**Optionen:**

| Flag | Beschreibung |
|------|-------------|
| `--server URL` | Server-URL (Standard: `http://localhost:3000`) |
| `--user NAME` | Benutzername (wird sonst interaktiv abgefragt) |
| `--yes` / `-y` | Alle Änderungen ohne Rückfrage synchronisieren |

**Ablauf:**
1. Anmeldung per Bearer-Token (`/api/auth/token`)
2. Server-Manifest laden (SHA-256-Checksummen)
3. Diff berechnen und anzeigen
4. Upload / Löschen durchführen
5. `/api/reload` – Server indiziert sofort neu

> Der Benutzer benötigt die Berechtigung **„Darf Dateien hochladen"** (im Admin-Panel setzbar). Der Admin hat sie automatisch.

---

## Konfiguration

### config.yml (vollständig)

```yaml
server:
  port: 3000
  vault_path: "/vault"
  site_title: "My Digital Garden"
  site_description: "A collection of interconnected notes"
  base_url: "http://localhost:3000"

database:
  url: "mysql://obsidian:changeme@mysql:3306/obsidian"

auth:
  session_secret: "64+ Zeichen langer Zufallsstring"
  session_duration_hours: 168   # 7 Tage

admin:
  username: "admin"
  password: "changeme"
```

### .env (Docker-Variablen)

| Variable               | Standard          | Beschreibung                          |
|------------------------|-------------------|---------------------------------------|
| `WEB_PORT`             | `3000`            | HTTP-Port des Web-Servers             |
| `MYSQL_ROOT_PASSWORD`  | `rootchangeme`    | MySQL-Root-Passwort                   |
| `MYSQL_PASS`           | `changeme`        | MySQL-Passwort für den App-Benutzer   |
| `RUST_LOG`             | `info`            | Log-Level (`debug`, `info`, `warn`)   |

---

## Benutzer & Berechtigungen

| Berechtigung         | Beschreibung                                    |
|----------------------|-------------------------------------------------|
| `is_admin`           | Vollzugriff; Berechtigungen nicht änderbar      |
| `can_upload`         | Darf Dateien per API hochladen                  |
| `can_manage_users`   | Kann Nutzer einladen und Rechte bearbeiten      |
| `can_manage_lists`   | Kann Whitelist / Blacklist bearbeiten           |
| `folder_restriction` | Beschränkt Zugriff auf bestimmte Ordner         |

Neue Benutzer werden per **Einladungslink** (48h gültig) registriert. Einladungslinks werden im Admin-Panel erstellt.

Der Admin-Benutzer kann weder bearbeitet noch gelöscht werden.

---

## Note-Frontmatter

```yaml
---
title: Mein Artikel
description: Eine kurze Zusammenfassung
tags: [biologie, abitur]
published: true
---
```

| Feld             | Bedeutung                                              |
|------------------|--------------------------------------------------------|
| `title`          | Angezeigter Titel (sonst Dateiname)                    |
| `description`    | Meta-Description und Vorschautext                      |
| `tags`           | Tag-Liste (Array oder YAML-Liste)                      |
| *(publish-feld)* | Beliebiger Name, im Admin-Panel konfigurierbar         |

---

## API-Endpunkte

### Öffentlich

| Endpunkt               | Beschreibung                                        |
|------------------------|-----------------------------------------------------|
| `GET /`                | Startseite / Weiterleitung zur ersten sichtbaren Note |
| `GET /note/:slug`      | Note rendern                                        |
| `GET /api/search?q=`   | Volltextsuche (JSON)                                |
| `GET /api/graph`       | Graph-Daten (Nodes + Links) als JSON                |
| `GET /static/`         | CSS, JS, Fonts                                      |
| `POST /api/auth/token` | Bearer-Token per Benutzername + Passwort            |
| `GET /auto-login`      | Token → Session-Cookie + Redirect (für Plugin)      |

### Authentifiziert (Session-Cookie oder Bearer-Token)

| Endpunkt                              | Berechtigung   | Beschreibung                              |
|---------------------------------------|----------------|-------------------------------------------|
| `GET /api/reload`                     | Upload         | Vault neu indizieren                      |
| `GET /api/files`                      | –              | Dateiliste (gefiltert nach Benutzer)      |
| `GET /api/files/manifest`             | Upload         | SHA-256-Checksummen aller Vault-Dateien   |
| `POST /api/upload`                    | Upload         | Datei hochladen (multipart/form-data)     |
| `DELETE /api/files/delete`            | Upload         | Datei löschen (`?path=pfad`)              |
| `GET /api/sync/download`              | –              | Datei herunterladen (`?path=pfad`)        |
| `POST /api/sync/device`               | –              | Gerät registrieren                        |
| `GET /admin`                          | Admin          | Admin-Panel (HTML)                        |
| `GET /api/admin/users`                | Nutzer-Mgmt    | Benutzerliste (JSON)                      |
| `POST /api/admin/users/:id/permissions` | Nutzer-Mgmt  | Berechtigungen aktualisieren              |
| `GET /api/admin/lists`                | Listen-Mgmt    | Whitelist + Blacklist (JSON)              |
| `POST /api/admin/lists`               | Listen-Mgmt    | Whitelist + Blacklist setzen              |
| `GET /api/admin/settings`             | Admin          | Server-Einstellungen (publish_field)      |
| `POST /api/admin/settings`            | Admin          | Server-Einstellungen speichern            |

---

## Projektstruktur

```
Server/
├── src/
│   ├── main.rs              # Axum-Server, Routen, AppState
│   ├── auth.rs              # Session-Middleware, CurrentUser
│   ├── config.rs            # Konfiguration aus config.yml
│   ├── db.rs                # MySQL via sqlx (Migrationen, CRUD)
│   ├── vault.rs             # Vault-Indizierung, Publish-Filter, Wikilinks
│   ├── parser/
│   │   ├── markdown.rs      # Markdown → HTML (comrak + MathJax)
│   │   ├── canvas.rs        # Obsidian-Canvas-Renderer
│   │   └── excalidraw.rs    # Excalidraw-Renderer
│   └── routes/
│       ├── note.rs          # GET /note/:slug, Dateibaum
│       ├── api.rs           # Upload, Manifest, Reload, Sync
│       ├── auth.rs          # Login, Logout, Register, Token, Auto-Login
│       └── admin.rs         # Admin-Panel, Nutzerverwaltung, Einstellungen
├── templates/
│   ├── base.html            # Layout (Sidebar, MathJax, D3)
│   ├── note.html            # Note-Ansicht
│   ├── index.html           # Startseite
│   ├── canvas.html          # Canvas-Ansicht
│   ├── excalidraw.html      # Excalidraw-Ansicht
│   ├── login.html           # Login-Seite
│   ├── register.html        # Registrierung per Einladungslink
│   ├── admin.html           # Admin-Panel
│   └── uploader.html        # Uploader-UI
├── migrations/
│   ├── 001_init.sql         # Benutzer, Sessions, Listen
│   ├── 002_sync.sql         # Vault-Dateien, Sync-Geräte
│   └── 003_settings.sql     # App-Einstellungen (publish_field)
├── static/
│   ├── css/style.css        # Design
│   ├── js/app.js            # Explorer, Dark Mode, Callouts
│   ├── js/search.js         # Volltextsuche
│   └── js/graph.js          # D3-Graph
├── config.yml.example       # Konfigurationsvorlage
├── Dockerfile               # Multi-Stage: rust → debian:bookworm-slim
├── docker-compose.yml       # Web-Server + MySQL
└── publish.py               # Upload-Script (HTTP API)
```

---

## Lokale Entwicklung (ohne Docker)

```bash
# Voraussetzungen: Rust ≥ 1.77 (rustup), laufende MySQL-Instanz

cp config.yml.example config.yml   # Datenbank-URL anpassen

cargo run                           # Server starten
RUST_LOG=debug cargo run            # Mit Debug-Logging
```

Server startet auf http://localhost:3000.

---

## Technologie-Stack

| Komponente       | Technologie                                                          |
|------------------|----------------------------------------------------------------------|
| Web-Server       | Rust · [Axum](https://github.com/tokio-rs/axum) 0.7                 |
| Templates        | [Tera](https://keats.github.io/tera/) (Jinja2-kompatibel)           |
| Markdown         | [comrak](https://github.com/kivikakk/comrak) (CommonMark + GFM)     |
| Math             | [MathJax](https://www.mathjax.org/) 3.2.2                            |
| Graph            | [D3.js](https://d3js.org/) 7.9                                       |
| Datenbank        | MySQL 8 via [sqlx](https://github.com/launchbadge/sqlx)              |
| Passwort-Hashing | [Argon2](https://docs.rs/argon2/)                                    |
| Containerisierung| Docker + Docker Compose                                              |
| Upload-Script    | Python 3 (nur Standardbibliothek)                                    |
