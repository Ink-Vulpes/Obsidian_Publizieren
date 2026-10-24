#!/usr/bin/env python3
"""
Obsidian Publisher – Upload-Skript
===================================
Vergleicht den lokalen Vault mit dem Server, zeigt neue / geänderte / gelöschte
Notizen und lädt die ausgewählten Dateien hoch.

Verwendung:
    python publish.py --vault C:/Users/mich/Obsidian/MeinVault
    python publish.py --vault /vault --server https://notes.example.com
    python publish.py /pfad/zum/vault           # positionales Argument auch OK

Optionen:
    --vault   Pfad zum lokalen Obsidian-Vault (oder als erstes posit. Argument)
    --server  Server-URL (Standard: http://localhost:3000)
    --user    Benutzername (wird sonst interaktiv abgefragt)
    --yes     Alle Änderungen ohne Rückfrage synchronisieren
"""

import argparse
import getpass
import hashlib
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


# ── Argumente ─────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Obsidian-Notizen mit dem Server synchronisieren."
    )
    p.add_argument(
        "vault_path",
        nargs="?",
        help="Pfad zum Obsidian-Vault (positionales Argument, alternativ zu --vault)",
    )
    p.add_argument("--vault",  metavar="PFAD", help="Pfad zum Obsidian-Vault")
    p.add_argument("--server", metavar="URL",  default=None,
                   help="Server-URL (Standard: http://localhost:3000)")
    p.add_argument("--user",   metavar="NAME", default=None,
                   help="Benutzername (wird sonst abgefragt)")
    p.add_argument("--yes", "-y", action="store_true",
                   help="Alle Änderungen ohne Rückfrage synchronisieren")
    return p.parse_args()


# ── SHA-256 ────────────────────────────────────────────────────────────────────

def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


# ── Vault scannen ──────────────────────────────────────────────────────────────

def scan_vault(vault: Path) -> dict[str, dict]:
    """Gibt {rel_pfad: {checksum, size}} für alle nicht-versteckten Dateien zurück."""
    result = {}
    for p in vault.rglob("*"):
        if not p.is_file():
            continue
        parts = p.relative_to(vault).parts
        if any(part.startswith(".") for part in parts):
            continue
        rel = "/".join(parts)
        try:
            result[rel] = {"checksum": sha256(p), "size": p.stat().st_size, "path": p}
        except OSError:
            pass
    return result


# ── HTTP-Hilfsfunktionen ───────────────────────────────────────────────────────

def api_get(server: str, path: str, token: str) -> list | dict:
    url = f"{server.rstrip('/')}{path}"
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


def api_post_json(server: str, path: str, data: dict, token: str = "") -> dict:
    url = f"{server.rstrip('/')}{path}"
    payload = json.dumps(data).encode()
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=payload, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        try:
            return json.loads(body)
        except Exception:
            return {"error": f"HTTP {e.code}: {body[:200]}"}


def upload_file(server: str, token: str, rel_path: str, local_path: Path) -> bool:
    """Lädt eine einzelne Datei via multipart/form-data hoch."""
    url = f"{server.rstrip('/')}/api/upload"
    data = local_path.read_bytes()
    boundary = "----ObsidianUpload"
    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{rel_path}"\r\n'
        f"Content-Type: application/octet-stream\r\n"
        f"x-file-path: {rel_path}\r\n"
        f"\r\n"
    ).encode() + data + f"\r\n--{boundary}--\r\n".encode()

    req = urllib.request.Request(
        url,
        data=body,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": f"multipart/form-data; boundary={boundary}",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status == 200
    except urllib.error.HTTPError as e:
        print(f"    [Fehler] {rel_path}: HTTP {e.code}")
        return False


def delete_remote(server: str, token: str, rel_path: str) -> bool:
    url = f"{server.rstrip('/')}/api/files/delete?path={urllib.parse.quote(rel_path)}"
    req = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {token}"},
        method="DELETE",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status == 200
    except urllib.error.HTTPError:
        return False


# ── Diff anzeigen ──────────────────────────────────────────────────────────────

GRÜN  = "\033[92m"
GELB  = "\033[93m"
ROT   = "\033[91m"
RESET = "\033[0m"
FETT  = "\033[1m"

def _colored(text: str, color: str) -> str:
    if sys.stdout.isatty():
        return f"{color}{text}{RESET}"
    return text


def show_diff(diff: list[dict]) -> None:
    new_files  = [d for d in diff if d["status"] == "new"]
    modified   = [d for d in diff if d["status"] == "modified"]
    deleted    = [d for d in diff if d["status"] == "deleted"]

    if new_files:
        print(f"\n{_colored('Neu', GRÜN)} ({len(new_files)}):")
        for d in new_files:
            size = f"  ({d['local_size']:,} B)" if d.get("local_size") else ""
            print(f"  + {d['path']}{size}")

    if modified:
        print(f"\n{_colored('Geändert', GELB)} ({len(modified)}):")
        for d in modified:
            size = f"  ({d['local_size']:,} B)" if d.get("local_size") else ""
            print(f"  ~ {d['path']}{size}")

    if deleted:
        print(f"\n{_colored('Gelöscht (nur auf Server)', ROT)} ({len(deleted)}):")
        for d in deleted:
            print(f"  - {d['path']}")


# ── Hauptprogramm ──────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()

    # Vault-Pfad ermitteln (--vault hat Vorrang, dann positionales Argument)
    vault_str = args.vault or args.vault_path
    if not vault_str:
        print("Fehler: Kein Vault-Pfad angegeben.\n"
              "  python publish.py --vault /pfad/zum/vault\n"
              "  python publish.py /pfad/zum/vault", file=sys.stderr)
        sys.exit(1)

    vault = Path(vault_str).resolve()
    if not vault.exists():
        print(f"Fehler: Vault-Pfad existiert nicht: {vault}", file=sys.stderr)
        sys.exit(1)

    # Server-URL aus .env lesen falls nicht angegeben
    server = args.server
    if not server:
        env_file = Path(__file__).parent / ".env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("BASE_URL="):
                    server = line.partition("=")[2].strip().strip('"\'')
        if not server:
            server = input("Server-URL [http://localhost:3000]: ").strip() or "http://localhost:3000"

    # Anmeldedaten
    username = args.user or input("Benutzername: ").strip()
    if not username:
        print("Fehler: Kein Benutzername angegeben.", file=sys.stderr)
        sys.exit(1)
    password = getpass.getpass("Passwort: ")

    # Login
    print(f"\nVerbinde mit {server} …")
    resp = api_post_json(server, "/api/auth/token",
                         {"username": username, "password": password})
    token = resp.get("token", "")
    if not token:
        print(f"Anmeldung fehlgeschlagen: {resp.get('error', 'Unbekannter Fehler')}", file=sys.stderr)
        sys.exit(1)
    print(f"Angemeldet als {_colored(resp.get('username', username), FETT)}")

    # Remote-Manifest laden
    print("Lade Server-Dateiliste …")
    try:
        remote_list = api_get(server, "/api/files/manifest", token)
    except Exception as e:
        print(f"Fehler beim Laden der Server-Dateiliste: {e}", file=sys.stderr)
        sys.exit(1)
    remote_map = {f["path"]: f["checksum"] for f in remote_list if "path" in f}

    # Lokalen Vault scannen
    print(f"Scanne Vault: {vault} …")
    local_map = scan_vault(vault)

    # Diff berechnen
    diff: list[dict] = []
    for rel, info in local_map.items():
        if rel not in remote_map:
            diff.append({"status": "new",      "path": rel, "local_size": info["size"]})
        elif info["checksum"] != remote_map[rel]:
            diff.append({"status": "modified", "path": rel, "local_size": info["size"]})
    for rel in remote_map:
        if rel not in local_map:
            diff.append({"status": "deleted",  "path": rel})

    diff.sort(key=lambda d: (d["status"], d["path"]))

    if not diff:
        print("\nKeine Änderungen — alles ist aktuell.")
        return

    show_diff(diff)

    upload_paths = [d["path"] for d in diff if d["status"] != "deleted"]
    delete_paths = [d["path"] for d in diff if d["status"] == "deleted"]

    print(f"\n{len(upload_paths)} Datei(en) hochladen, "
          f"{len(delete_paths)} Datei(en) löschen.")

    if not args.yes:
        try:
            choice = input("\n[A]lle / [N]ichte / manuell auswählen [a/n/m]? ").strip().lower()
        except KeyboardInterrupt:
            print("\nAbgebrochen.")
            return

        if choice == "n":
            print("Abgebrochen.")
            return
        elif choice == "m":
            # Manuelle Auswahl
            selected_upload, selected_delete = [], []
            for d in diff:
                action = "hochladen" if d["status"] != "deleted" else "löschen"
                yn = input(f"  {d['status']:8}  {d['path']}  [{action}]? [j/n] ").strip().lower()
                if yn == "j":
                    if d["status"] == "deleted":
                        selected_delete.append(d["path"])
                    else:
                        selected_upload.append(d["path"])
            upload_paths = selected_upload
            delete_paths = selected_delete
        # else "a" oder Enter → alle

    # Dateien hochladen
    uploaded, errors = 0, 0
    for rel in upload_paths:
        local_path = vault / Path(rel)
        print(f"  ↑ {rel}", end=" … ", flush=True)
        if upload_file(server, token, rel, local_path):
            print("OK")
            uploaded += 1
        else:
            print("FEHLER")
            errors += 1

    # Dateien löschen
    deleted_count = 0
    for rel in delete_paths:
        print(f"  ✕ {rel}", end=" … ", flush=True)
        if delete_remote(server, token, rel):
            print("OK")
            deleted_count += 1
        else:
            print("FEHLER")
            errors += 1

    # Server-Reload
    if uploaded or deleted_count:
        print("\nServer neu laden …")
        try:
            api_get(server, "/api/reload", token)
            print("OK")
        except Exception:
            pass

    print(f"\n✓ {uploaded} hochgeladen, {deleted_count} gelöscht"
          + (f", {errors} Fehler" if errors else "") + ".")


if __name__ == "__main__":
    main()
