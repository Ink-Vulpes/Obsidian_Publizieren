# Obsidian Publisher

Ein selbst gehosteter Web-Server, der eine Obsidian-Vault live im Browser rendert – ähnlich wie [Quartz](https://quartz.jzhao.xyz/), aber vollständig in Rust geschrieben und per FTP bespielbar.

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
- **FTP-Upload** via `publish.py` – nur Notes mit `Publiziert: true` im Frontmatter werden hochgeladen, inklusive eingebetteter Assets

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

### 2. Umgebungsvariablen konfigurieren

```bash
cp .env.example .env   # oder .env direkt bearbeiten
```

Wichtige Einstellungen in `.env`:

```env
SITE_TITLE=Mein Digital Garden
SITE_DESCRIPTION=Meine vernetzten Notizen
BASE_URL=http://localhost:3000

FTP_USER=obsidian
FTP_PASS=sicheres-passwort
PASV_ADDRESS=127.0.0.1   # öffentliche IP für externen Zugriff
```

### 3. Container starten

```bash
docker compose up -d
```

Der Web-Server läuft auf **http://localhost:3000**.  
Der FTP-Server lauscht auf **Port 21** (Daten: 21100–21110).

### 4. Notes hochladen

```bash
python publish.py /pfad/zur/obsidian-vault
```

Das Script:
1. Findet alle `.md`-Dateien mit `Publiziert: true` im Frontmatter
2. Sammelt eingebettete Bilder, PDFs und andere Assets
3. Wendet `whitelist.json` / `blacklist.json` an
4. Lädt alles per FTP hoch
5. Ruft `/api/reload` auf – der Server indiziert sofort neu

---

## Konfiguration

### Umgebungsvariablen (vollständig)

| Variable           | Standard              | Beschreibung                          |
|--------------------|-----------------------|---------------------------------------|
| `VAULT_PATH`       | `/vault`              | Pfad zur Vault im Container           |
| `WEB_PORT`         | `3000`                | HTTP-Port des Web-Servers             |
| `SITE_TITLE`       | `My Digital Garden`   | Seitentitel                           |
| `SITE_DESCRIPTION` | *(leer)*              | Meta-Description                      |
| `BASE_URL`         | `http://localhost:3000` | Öffentliche URL                     |
| `FTP_USER`         | `obsidian`            | FTP-Benutzername                      |
| `FTP_PASS`         | `changeme`            | FTP-Passwort                          |
| `PASV_ADDRESS`     | `127.0.0.1`           | IP für FTP-Passiv-Modus               |
| `FTP_HOST`         | `localhost`           | FTP-Host (für `publish.py`)           |
| `FTP_PORT`         | `21`                  | FTP-Port (für `publish.py`)           |
| `RUST_LOG`         | `info`                | Log-Level (`debug`, `info`, `warn`)   |

### whitelist.json / blacklist.json

Glob-Muster relativ zur Vault-Wurzel:

```json
["Templates/**", "Assets/logo.png", "**/*.png"]
```

| Muster            | Bedeutung                              |
|-------------------|----------------------------------------|
| `Ordner/**`       | Alles in `Ordner/` (beliebige Tiefe)   |
| `**/*.pdf`        | Alle PDF-Dateien überall              |
| `Notes/Artikel.md`| Exakt diese Datei                     |
| `*.md`            | Nur `.md` im Vault-Wurzelverzeichnis  |

**Whitelist**: Dateien werden hochgeladen, auch ohne `Publiziert: true`.  
**Blacklist**: Dateien werden niemals hochgeladen (auch wenn published).

---

## Note-Frontmatter

```yaml
---
title: Mein Artikel
description: Eine kurze Zusammenfassung
tags: [biologie, abitur]
Publiziert: true
---
```

| Feld           | Bedeutung                                      |
|----------------|------------------------------------------------|
| `title`        | Angezeigter Titel (sonst Dateiname)            |
| `description`  | Meta-Description und Vorschautext              |
| `tags`         | Tag-Liste (Array oder YAML-Liste)              |
| `Publiziert`   | `true` → wird von `publish.py` hochgeladen     |

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

## Projektstruktur

```
obsidian-publisher/
├── src/
│   ├── main.rs              # Axum-Server, Routen, Vault-State
│   ├── config.rs            # Konfiguration aus Umgebungsvariablen
│   ├── vault.rs             # Vault-Indizierung, Wikilink-Auflösung
│   ├── parser/
│   │   ├── markdown.rs      # Markdown → HTML (comrak + MathJax-Schutz)
│   │   ├── canvas.rs        # Obsidian-Canvas-Renderer
│   │   └── excalidraw.rs    # Excalidraw-Renderer
│   └── routes/
│       ├── note.rs          # GET /note/:slug
│       ├── api.rs           # GET /api/reload, /api/search, /api/graph
│       └── mod.rs
├── templates/
│   ├── base.html            # Layout mit Sidebar, MathJax, D3
│   ├── note.html            # Note-Ansicht
│   ├── index.html           # Startseite
│   ├── canvas.html          # Canvas-Ansicht
│   └── excalidraw.html      # Excalidraw-Ansicht
├── static/
│   ├── css/style.css        # Quartz-inspiriertes Design
│   ├── js/app.js            # Explorer, Dark Mode, Callouts
│   ├── js/search.js         # Volltextsuche
│   └── js/graph.js          # D3-Graphen
├── Dockerfile               # Multi-Stage: rust:1.88-slim → debian:bookworm-slim
├── docker-compose.yml       # Web-Server + vsftpd
├── publish.py               # Upload-Script
├── whitelist.json           # Glob-Muster für Force-Include
└── blacklist.json           # Glob-Muster für Force-Exclude
```

---

## Lokale Entwicklung (ohne Docker)

```bash
# Abhängigkeiten: Rust (rustup), cargo

# Vault-Pfad setzen und Server starten
VAULT_PATH=/pfad/zur/vault cargo run

# Mit allen Optionen
VAULT_PATH=/pfad/zur/vault \
SITE_TITLE="Mein Garden" \
RUST_LOG=debug \
cargo run
```

Server startet auf http://localhost:3000.  
Nach Dateiänderungen in der Vault: http://localhost:3000/api/reload aufrufen.

---

## API-Endpunkte

| Endpunkt            | Beschreibung                                        |
|---------------------|-----------------------------------------------------|
| `GET /`             | Startseite (Vault-Übersicht)                       |
| `GET /note/:slug`   | Note rendern (Slug = Vault-Pfad ohne Erweiterung)  |
| `GET /api/reload`   | Vault neu indizieren                               |
| `GET /api/search?q=`| Volltextsuche, gibt JSON zurück                    |
| `GET /api/graph`    | Graph-Daten (Nodes + Links) als JSON               |
| `GET /vault-assets/`| Statische Vault-Assets (Bilder, PDFs, …)           |
| `GET /static/`      | CSS, JS, Fonts                                     |

---

## Technologie-Stack

| Komponente    | Technologie                              |
|---------------|------------------------------------------|
| Web-Server    | Rust · [Axum](https://github.com/tokio-rs/axum) 0.7 |
| Templates     | [Tera](https://keats.github.io/tera/) (Jinja2-kompatibel) |
| Markdown      | [comrak](https://github.com/kivikakk/comrak) 0.21 (CommonMark + GFM) |
| Math          | [MathJax](https://www.mathjax.org/) 3.2.2 |
| Graph         | [D3.js](https://d3js.org/) 7.9           |
| FTP-Server    | vsftpd (im Docker-Container)             |
| Upload-Script | Python 3 (nur `ftplib`, keine externen Deps) |
