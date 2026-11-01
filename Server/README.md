# Obsidian Publisher — Server

A self-hosted web server that renders an Obsidian vault live in the browser — similar to [Quartz](https://quartz.jzhao.xyz/), but written entirely in Rust with MySQL authentication and HTTP upload.

![Rust](https://img.shields.io/badge/built_with-Rust-orange) ![Docker](https://img.shields.io/badge/deploy-Docker-blue)

---

## Features

- **3-column layout** (Explorer · Content · TOC/Backlinks/Graph) with dark/light mode
- **Obsidian wikilinks** `[[Note]]` and `[[Note|Alias]]` resolved to clickable links
- **Backlinks** — which notes link to the current one?
- **Local D3 graph** — relationships between notes visualized
- **Full-text search** (Ctrl+K) across all published notes
- **LaTeX / MathJax 3** — `$inline$` and `$$block$$` rendered
- **Image embeds** `![[image.png]]` as `<img>`, path resolved vault-wide
- **PDF links** open the file directly in the browser
- **Callouts** (`> [!info]`, `> [!warning]`, etc.) in Obsidian style
- **Canvas** (`.canvas`) and **Excalidraw** (`.excalidraw`) rendered with white background
- **Obsidian Bases** (`.base`) rendered as filterable/sortable tables
- **Tags**, **table of contents**, **anchor links** on headings
- **Metadata sidebar** — frontmatter values shown as clickable links (wikilinks and Markdown links)
- **JWT authentication** — login, invitation links, roles and permissions via admin panel
- **Groups** — assign notes to groups and restrict user access per group
- **Publish-field filter** — only notes with a configurable frontmatter field set to `true` are visible to non-admins
- **HTTP upload** via Obsidian plugin — checksum-based sync (SHA-256)
- **Auto-login** — one-time token link for seamless redirect from the Obsidian plugin

---

## Quick start with Docker

### Prerequisites

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (Windows / Mac / Linux)

### 1. Clone the repository

```bash
git clone https://github.com/Ink-Vulpes/Obsidian_Publizieren.git
cd Server
```

### 2. Create configuration

```bash
cp config.yml.example config.yml
```

Key settings in `config.yml`:

```yaml
server:
  port: 3000
  vault_path: "/vault"
  site_title: "My Digital Garden"
  base_url: "http://localhost:3000"

database:
  url: "mysql://obsidian:changeme@mysql:3306/obsidian"

auth:
  session_secret: "AT_LEAST_64_RANDOM_CHARACTERS_HERE!!!"
  session_duration_hours: 168

admin:
  username: "admin"
  password: "changeme"   # change on first start!
```

Optional overrides via `.env` file:

```env
WEB_PORT=3000
MYSQL_ROOT_PASSWORD=secure-root-password
MYSQL_PASS=secure-db-password
RUST_LOG=info
```

### 3. Start containers

```bash
docker compose up -d
```

The web server runs at **http://localhost:3000**.

On first start:
1. Database migrations run automatically.
2. The admin user (from `config.yml`) is created if no user exists yet.
3. The vault is indexed.

### 4. Admin panel

At **http://localhost:3000/admin** (after login):

| Tab | Function |
|-----|----------|
| **Users** | View users, edit permissions, create invitation links |
| **Groups** | Create groups and assign notes to them |
| **Whitelist / Blacklist** | Control visibility of individual notes |
| **Settings** | Configure publish field and group field |

### 5. Configure the publish field

In the admin panel → **Settings**, set a frontmatter field as the publish toggle (default: `published`).

Notes with `published: true` (or the configured field name) are visible to all users. Notes without this field or with `false` are only visible to admins. Leaving the field empty makes all notes visible to everyone.

Example frontmatter:

```yaml
---
published: true
title: My Article
---
```

---

## Deploying the Docker image

The pre-built image is available on the **GitHub Container Registry** at `ghcr.io/ink-vulpes/obsidian-publisher`.

### Deploy on a server

```bash
echo "YOUR_TOKEN" | docker login ghcr.io -u Ink-Vulpes --password-stdin
docker compose pull
docker compose up -d
```

Update to the latest version:

```bash
docker compose pull && docker compose up -d
```

---

## Configuration reference

### config.yml (complete)

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
  session_secret: "64+ character random string"
  session_duration_hours: 168   # 7 days

admin:
  username: "admin"
  password: "changeme"
```

### .env (Docker variables)

| Variable               | Default           | Description                              |
|------------------------|-------------------|------------------------------------------|
| `WEB_PORT`             | `3000`            | HTTP port of the web server              |
| `MYSQL_ROOT_PASSWORD`  | `rootchangeme`    | MySQL root password                      |
| `MYSQL_PASS`           | `changeme`        | MySQL password for the app user          |
| `RUST_LOG`             | `info`            | Log level (`debug`, `info`, `warn`)      |

---

## Users & permissions

| Permission           | Description                                           |
|----------------------|-------------------------------------------------------|
| `is_admin`           | Full access; permissions cannot be changed            |
| `can_upload`         | May upload files via the API                          |
| `can_manage_users`   | Can invite users and edit their permissions           |
| `can_manage_lists`   | Can edit whitelist / blacklist                        |
| `folder_restriction` | Restricts access to specific folders                  |
| `sees_published`     | Sees all published notes, ignoring group membership   |
| `sees_all`           | Sees everything including unpublished notes           |

New users are registered via **invitation links** (valid for 48 hours), created in the admin panel.

The admin user can neither be edited nor deleted.

---

## Groups

Groups control which notes a user can see beyond publicly published ones.

- Each note can have a `group` frontmatter field (configurable in Settings).
- Users are assigned to one or more groups in the admin panel.
- A user without `sees_published` or `sees_all` only sees notes whose group they belong to (plus publicly published notes).

---

## Note frontmatter

```yaml
---
title: My Article
description: A short summary
tags: [biology, science]
published: true
group: biology-students
---
```

| Field            | Purpose                                                 |
|------------------|---------------------------------------------------------|
| `title`          | Displayed title (falls back to filename)                |
| `description`    | Meta description and preview text                       |
| `tags`           | Tag list (array or YAML list)                           |
| *(publish field)*| Configurable name; set in admin panel                   |
| *(group field)*  | Configurable name; controls group visibility            |

---

## API reference

### Public

| Endpoint               | Description                                          |
|------------------------|------------------------------------------------------|
| `GET /`                | Homepage / redirect to first visible note            |
| `GET /note/:slug`      | Render a note                                        |
| `GET /api/search?q=`   | Full-text search (JSON)                              |
| `GET /api/graph`       | Graph data (nodes + links) as JSON                   |
| `GET /static/`         | CSS, JS, fonts                                       |
| `POST /api/auth/token` | Bearer token via username + password                 |
| `GET /auto-login`      | Token → session cookie + redirect (for plugin)       |

### Authenticated (session cookie or bearer token)

| Endpoint                                  | Permission     | Description                               |
|-------------------------------------------|----------------|-------------------------------------------|
| `GET /api/reload`                         | Upload         | Re-index vault                            |
| `GET /api/files`                          | —              | File list (filtered by user)              |
| `GET /api/files/manifest`                 | Upload         | SHA-256 checksums of all vault files      |
| `POST /api/upload`                        | Upload         | Upload a file (multipart/form-data)       |
| `DELETE /api/files/delete`                | Upload         | Delete a file (`?path=…`)                 |
| `GET /api/sync/download`                  | —              | Download a file (`?path=…`)               |
| `POST /api/sync/device`                   | —              | Register a device                         |
| `GET /admin`                              | Admin          | Admin panel (HTML)                        |
| `GET /api/admin/users`                    | User mgmt      | User list (JSON)                          |
| `POST /api/admin/users/:id/permissions`   | User mgmt      | Update permissions                        |
| `DELETE /api/admin/users/:id`             | Admin          | Delete a user                             |
| `GET /api/admin/lists`                    | List mgmt      | Whitelist + blacklist (JSON)              |
| `POST /api/admin/lists`                   | List mgmt      | Set whitelist + blacklist                 |
| `GET /api/admin/settings`                 | Admin          | Server settings (JSON)                    |
| `POST /api/admin/settings`                | Admin          | Save server settings                      |
| `GET /api/admin/groups`                   | Admin          | List groups with members (JSON)           |
| `POST /api/admin/groups`                  | Admin          | Create a group                            |
| `DELETE /api/admin/groups/:id`            | Admin          | Delete a group                            |
| `POST /api/admin/groups/:id/members`      | Admin          | Add user to group                         |
| `DELETE /api/admin/groups/:id/members/:uid` | Admin        | Remove user from group                    |

---

## Project structure

```
Server/
├── src/
│   ├── main.rs              # Axum server, routes, AppState
│   ├── auth.rs              # JWT middleware, CurrentUser
│   ├── config.rs            # Configuration from config.yml
│   ├── db.rs                # MySQL via sqlx (migrations, CRUD)
│   ├── vault.rs             # Vault indexing, publish filter, wikilinks
│   ├── external_meta.rs     # External metadata overrides (groups, etc.)
│   └── parser/
│       ├── markdown.rs      # Markdown → HTML (comrak + MathJax)
│       ├── bases.rs         # Obsidian Bases renderer
│       ├── canvas.rs        # Obsidian Canvas renderer
│       └── excalidraw.rs    # Excalidraw data extractor
│   └── routes/
│       ├── note.rs          # GET /note/:slug, file tree
│       ├── api.rs           # Upload, manifest, reload, sync
│       ├── auth.rs          # Login, logout, register, token, auto-login
│       └── admin.rs         # Admin panel, user management, settings, groups
├── templates/
│   ├── base.html            # Layout (sidebar, MathJax, D3)
│   ├── note.html            # Note view
│   ├── bases.html           # Bases view
│   ├── canvas.html          # Canvas view
│   ├── excalidraw.html      # Excalidraw view
│   ├── login.html           # Login page
│   ├── register.html        # Registration via invitation link
│   └── admin.html           # Admin panel
├── migrations/
│   ├── 001_init.sql         # Users, sessions, lists
│   ├── 002_sync.sql         # Vault files, sync devices
│   ├── 003_settings.sql     # App settings (publish_field)
│   ├── 004_jwt.sql          # JWT token storage
│   ├── 005_groups.sql       # Groups and group membership
│   └── 006_user_visibility.sql  # sees_published / sees_all permissions
├── static/
│   ├── css/style.css        # Design
│   ├── js/app.js            # Explorer, dark mode, metadata links
│   ├── js/search.js         # Full-text search
│   ├── js/graph.js          # D3 graph
│   ├── js/excalidraw-embed.js   # Excalidraw renderer (embeds + pages)
│   └── js/excalidraw-page.js    # Standalone Excalidraw page renderer
├── config.yml.example       # Configuration template
├── Dockerfile               # Multi-stage: rust → debian:bookworm-slim
└── docker-compose.yml       # Web server + MySQL
```

---

## Local development (without Docker)

```bash
# Prerequisites: Rust ≥ 1.77 (rustup), running MySQL instance

cp config.yml.example config.yml   # adjust database URL

cargo run                           # start server
RUST_LOG=debug cargo run            # with debug logging
```

Server starts at http://localhost:3000.

---

## Tech stack

| Component        | Technology                                                           |
|------------------|----------------------------------------------------------------------|
| Web server       | Rust · [Axum](https://github.com/tokio-rs/axum) 0.7                 |
| Templates        | [Tera](https://keats.github.io/tera/) (Jinja2-compatible)           |
| Markdown         | [comrak](https://github.com/kivikakk/comrak) (CommonMark + GFM)     |
| Math             | [MathJax](https://www.mathjax.org/) 3.2.2                            |
| Graph            | [D3.js](https://d3js.org/) 7.9                                       |
| Database         | MySQL 8 via [sqlx](https://github.com/launchbadge/sqlx)              |
| Password hashing | [Argon2](https://docs.rs/argon2/)                                    |
| Containerization | Docker + Docker Compose                                              |
