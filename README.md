# Obsidian Publisher

Ein selbst gehosteter Web-Server, der eine Obsidian-Vault live im Browser rendert – ähnlich wie [Quartz](https://quartz.jzhao.xyz/), aber vollständig in Rust geschrieben mit MySQL-Authentifizierung und HTTP-Upload.

![Layout](https://img.shields.io/badge/layout-3--column-blue) ![Rust](https://img.shields.io/badge/built_with-Rust-orange) ![Docker](https://img.shields.io/badge/deploy-Docker-blue)

---

## Features

- **3-Spalten-Layout** (Explorer · Inhalt · TOC/Backlinks/Graph) mit Dark/Light-Mode
- **Obsidian-Wikilinks** `[[Note]]` und `[[Note|Alias]]` werden zu klickbaren Links aufgelöst
- **Backlinks** – welche Notes verweisen auf die aktuelle?
- **Lokaler D3-Graph** – Beziehungen zwischen Notes visualisiert
- **Volltextsuche** (Ctrl+K) über alle veröffentlichten Notes
- **LaTeX / MathJax 3** – `$inline$` und `$$block$$` werden gerendert
- **Bild-Embeds** `![[bild.png]]` werden als `<img>` eingebettet, Pfad wird vault-weit aufgelöst
- **PDF-Links** `[[doc.pdf]]` und `[Text](doc.pdf)` öffnen die Datei im Browser
- **Callouts** (`> [!info]`, `> [!warning]` usw.) in Obsidian-Stil
- **Canvas** (`.canvas`) und **Excalidraw** (`.excalidraw` / Plugin-Markdown) werden gerendert
- **Tags**, **Inhaltsverzeichnis**, **Anker-Links** an Überschriften
- **Authentifizierung** – Login, Einladungslinks, Rollen und Berechtigungen per Admin-Panel
- **HTTP-Upload** via `publish.py` – Checksummen-basierter Sync (SHA-256), kein FTP nötig

---

## Schnellstart mit Docker

### Voraussetzungen

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (Windows/Mac/Linux)
- Python 3.9+ (nur für `publish.py`)

### 1. Repository klonen

```bash
git clone <repo-url>
cd obsidian-publisher
```

### 2. Konfiguration anlegen

```bash
cp config.yml.example config/config.yml
```

Wichtige Einstellungen in `config/config.yml`:

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
  password: "changeme"   # beim ersten Start geändert werden!
```

Optionale Anpassungen per `.env`-Datei im Projekt-Wurzelverzeichnis:

```env
WEB_PORT=3000
FTP_USER=obsidian
FTP_PASS=sicheres-passwort
PASV_ADDRESS=127.0.0.1
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
1. Die Datenbank-Migrationen werden automatisch ausgeführt.
2. Der Admin-Benutzer (aus `config.yml`) wird angelegt, sofern noch kein Benutzer existiert.
3. Der Vault wird indiziert.

### 4. Admin-Panel

Unter **http://localhost:3000/admin** (nach Login) können:

- **Benutzer** verwaltet und Einladungslinks (48h gültig) erstellt werden
- **Berechtigungen** je Nutzer gesetzt werden (Upload, Nutzerverwaltung, Listen)
- **Whitelist / Blacklist** für Notes gepflegt werden

### 5. Notes hochladen

```bash
python publish.py --vault /pfad/zur/obsidian-vault
```

Optionen:

| Flag | Beschreibung |
|------|-------------|
| `--vault PFAD` | Pfad zum Obsidian-Vault (auch als erstes positionelles Argument) |
| `--server URL` | Server-URL (Standard: `http://localhost:3000`) |
| `--user NAME` | Benutzername (wird sonst interaktiv abgefragt) |
| `--yes` / `-y` | Alle Änderungen ohne Rückfrage synchronisieren |

Das Script:
1. Meldet sich per Bearer-Token an (`/api/auth/token`)
2. Lädt das Server-Manifest (`/api/files/manifest`) mit SHA-256-Checksummen
3. Scannt den lokalen Vault und berechnet lokale Checksummen
4. Zeigt neue / geänderte / gelöschte Dateien an
5. Lädt ausgewählte Dateien per `multipart/form-data` hoch
6. Löscht entfernte Dateien auf dem Server
7. Ruft `/api/reload` auf – der Server indiziert sofort neu

> **Hinweis (Windows / PowerShell):** Wenn der Vault-Pfad mit `\` endet, kommt es zu einem Escaping-Problem. Entweder den Pfad ohne abschließenden Backslash angeben oder Anführungszeichen weglassen:
> ```powershell
> python publish.py --vault "D:\My Second Brain"
> ```

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
| `FTP_USER`             | `obsidian`        | FTP-Benutzername (vsftpd)             |
| `FTP_PASS`             | `changeme`        | FTP-Passwort (vsftpd)                 |
| `PASV_ADDRESS`         | `127.0.0.1`       | IP für FTP-Passiv-Modus               |
| `MYSQL_ROOT_PASSWORD`  | `rootchangeme`    | MySQL-Root-Passwort                   |
| `MYSQL_PASS`           | `changeme`        | MySQL-Passwort für den App-Benutzer   |
| `RUST_LOG`             | `info`            | Log-Level (`debug`, `info`, `warn`)   |

---

## Benutzer & Berechtigungen

Das System unterstützt mehrere Benutzer mit feingranularen Rechten:

| Berechtigung         | Beschreibung                                    |
|----------------------|-------------------------------------------------|
| `is_admin`           | Vollzugriff, kann Nutzer löschen                |
| `can_upload`         | Darf Dateien per API hochladen                  |
| `can_manage_users`   | Kann Nutzer einladen und Rechte bearbeiten      |
| `can_manage_lists`   | Kann Whitelist / Blacklist bearbeiten           |
| `folder_restriction` | Beschränkt Upload auf bestimmte Ordner          |

Neue Benutzer werden per **Einladungslink** (48h gültig) registriert. Der Link wird im Admin-Panel unter „Einladungslink erstellen" generiert.

---

## Note-Frontmatter

```yaml
---
title: Mein Artikel
description: Eine kurze Zusammenfassung
tags: [biologie, abitur]
---
```

| Feld          | Bedeutung                                      |
|---------------|------------------------------------------------|
| `title`       | Angezeigter Titel (sonst Dateiname)            |
| `description` | Meta-Description und Vorschautext              |
| `tags`        | Tag-Liste (Array oder YAML-Liste)              |

> Das Feld `Publiziert: true` aus früheren Versionen wird nicht mehr ausgewertet. `publish.py` lädt alle Dateien des Vaults hoch (außer versteckten Verzeichnissen); Sichtbarkeit wird über Whitelist/Blacklist im Admin-Panel gesteuert.

---

## LaTeX / Mathematik

Obsidian-Syntax wird unterstützt:

```markdown
Inline: $E = mc^2$

Block:
$$
\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$
```

MathJax 3.2.2 rendert die Formeln im Browser.

---

## API-Endpunkte

### Öffentlich

| Endpunkt              | Beschreibung                                        |
|-----------------------|-----------------------------------------------------|
| `GET /`               | Startseite (Vault-Übersicht)                        |
| `GET /note/:slug`     | Note rendern (Slug = Vault-Pfad ohne Erweiterung)   |
| `GET /api/search?q=`  | Volltextsuche, gibt JSON zurück                     |
| `GET /api/graph`      | Graph-Daten (Nodes + Links) als JSON                |
| `GET /vault-assets/`  | Statische Vault-Assets (Bilder, PDFs, …)            |
| `GET /static/`        | CSS, JS, Fonts                                      |
| `POST /api/auth/token`| Bearer-Token per Benutzername + Passwort            |

### Authentifiziert (Session-Cookie oder Bearer-Token)

| Endpunkt                    | Beschreibung                              |
|-----------------------------|-------------------------------------------|
| `GET /api/reload`           | Vault neu indizieren                      |
| `GET /api/files/manifest`   | Dateiliste mit SHA-256-Checksummen        |
| `POST /api/upload`          | Datei hochladen (multipart/form-data)     |
| `DELETE /api/files/delete`  | Datei löschen (`?path=relativer/pfad`)    |
| `GET /admin`                | Admin-Panel (benötigt Admin-Berechtigung) |

---

## Projektstruktur

```
obsidian-publisher/
├── src/
│   ├── main.rs              # Axum-Server, Routen, Vault-State
│   ├── auth.rs              # Session-Middleware, CurrentUser, Berechtigungen
│   ├── config.rs            # Konfiguration aus config.yml
│   ├── db.rs                # MySQL-Verbindung via sqlx
│   ├── vault.rs             # Vault-Indizierung, Wikilink-Auflösung
│   ├── parser/
│   │   ├── markdown.rs      # Markdown → HTML (comrak + MathJax-Schutz)
│   │   ├── canvas.rs        # Obsidian-Canvas-Renderer
│   │   └── excalidraw.rs    # Excalidraw-Renderer
│   ├── routes/
│   │   ├── note.rs          # GET /note/:slug
│   │   ├── api.rs           # /api/reload, /api/search, /api/graph, Upload, Manifest
│   │   ├── auth.rs          # Login, Logout, Register, Token-Endpunkt
│   │   ├── admin.rs         # Admin-Panel, Nutzerverwaltung, White-/Blacklist
│   │   └── mod.rs
│   └── bin/
│       └── uploader.rs      # Lokale Uploader-UI (obsidian-uploader Binary)
├── templates/
│   ├── base.html            # Layout mit Sidebar, MathJax, D3
│   ├── note.html            # Note-Ansicht
│   ├── index.html           # Startseite
│   ├── canvas.html          # Canvas-Ansicht
│   ├── excalidraw.html      # Excalidraw-Ansicht
│   ├── login.html           # Login-Seite
│   ├── register.html        # Registrierungs-Seite (Einladungslink)
│   └── admin.html           # Admin-Panel
├── static/
│   ├── css/style.css        # Quartz-inspiriertes Design
│   ├── js/app.js            # Explorer, Dark Mode, Callouts
│   ├── js/search.js         # Volltextsuche
│   └── js/graph.js          # D3-Graphen
├── migrations/              # SQL-Migrationen (sqlx)
├── config/
│   └── config.yml           # Serverkonfiguration (nicht im Repo)
├── Dockerfile               # Multi-Stage: rust:1.89-slim → debian:bookworm-slim
├── docker-compose.yml       # Web-Server + MySQL + vsftpd
└── publish.py               # Upload-Script (HTTP API, kein FTP)
```

---

## Lokale Entwicklung (ohne Docker)

```bash
# Abhängigkeiten: Rust ≥ 1.89 (rustup), eine laufende MySQL-Instanz

# Konfiguration anpassen
cp config.yml.example config/config.yml   # DATABASE_URL etc. setzen

# Server starten
CONFIG_PATH=config/config.yml cargo run

# Mit Debug-Logging
CONFIG_PATH=config/config.yml RUST_LOG=debug cargo run
```

Server startet auf http://localhost:3000.  
Nach Dateiänderungen in der Vault: `GET /api/reload` aufrufen oder `publish.py` verwenden.

---

## Technologie-Stack

| Komponente     | Technologie                              |
|----------------|------------------------------------------|
| Web-Server     | Rust · [Axum](https://github.com/tokio-rs/axum) 0.7 |
| Templates      | [Tera](https://keats.github.io/tera/) (Jinja2-kompatibel) |
| Markdown       | [comrak](https://github.com/kivikakk/comrak) (CommonMark + GFM) |
| Math           | [MathJax](https://www.mathjax.org/) 3.2.2 |
| Graph          | [D3.js](https://d3js.org/) 7.9           |
| Datenbank      | MySQL 8 via [sqlx](https://github.com/launchbadge/sqlx) |
| Passwort-Hashing | [bcrypt](https://docs.rs/bcrypt/)      |
| Sessions       | HTTP-only Cookies (AES-GCM verschlüsselt) |
| FTP-Server     | vsftpd (optional, im Docker-Container)   |
| Upload-Script  | Python 3 (nur Standardbibliothek)        |
