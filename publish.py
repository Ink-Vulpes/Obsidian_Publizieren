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
            if r.status != 200:
                return False
            try:
                body = json.loads(r.read())
                if body.get("errors"):
                    for err in body["errors"]:
                        sys.stdout.write(f"\n  {_colored('FEHLER', ROT)} server: {err}\n")
                        sys.stdout.flush()
                return body.get("uploaded", 0) > 0
            except Exception:
                return True
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
CYAN  = "\033[96m"

def _colored(text: str, color: str) -> str:
    if sys.stdout.isatty():
        return f"{color}{text}{RESET}"
    return text


# ── Fortschrittsbalken ─────────────────────────────────────────────────────────

def _fmt_bytes(n: int) -> str:
    if n >= 1_073_741_824:
        return f"{n / 1_073_741_824:.2f} GB"
    if n >= 1_048_576:
        return f"{n / 1_048_576:.1f} MB"
    return f"{n / 1024:.0f} KB"


def print_progress(done: int, total: int, done_bytes: int, total_bytes: int,
                   current: str, errors: int) -> None:
    """Überschreibt die aktuelle Zeile mit Balken + Statistik."""
    if not sys.stdout.isatty():
        return
    width = 30
    filled = int(width * done / total) if total else width
    bar = "█" * filled + "░" * (width - filled)
    pct = int(100 * done / total) if total else 100
    name = current[-38:] if len(current) > 38 else current
    err_str = f"  {_colored(f'{errors} Fehler', ROT)}" if errors else ""
    line = (
        f"\r  {_colored(bar, CYAN)}  {pct:3d}%  "
        f"{done}/{total} Dateien  "
        f"{_colored(_fmt_bytes(done_bytes), GRÜN)}/{_fmt_bytes(total_bytes)}"
        f"{err_str}  {name:<38}"
    )
    sys.stdout.write(line)
    sys.stdout.flush()


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

    # PowerShell W32 arg-passing bug: 'C:\path\' --server X arrives as 'C:\path" --server X'
    # Split at the first stray " and discard everything after it.
    if '"' in vault_str:
        vault_str = vault_str.split('"')[0]
    vault_str = vault_str.strip("'").rstrip('\\').rstrip('/')
    vault = Path(vault_str).resolve()
    if not vault.exists():
        print(f"Fehler: Vault-Pfad existiert nicht: {vault}", file=sys.stderr)
        sys.exit(1)

    # Server-URL ermitteln und Protokoll sicherstellen
    server = args.server
    if not server:
        env_file = Path(__file__).parent / ".env"
        if env_file.exists():
            for line in env_file.read_text().splitlines():
                if line.startswith("BASE_URL="):
                    server = line.partition("=")[2].strip().strip('"\'')
        if not server:
            server = input("Server-URL [http://localhost:3000]: ").strip() or "http://localhost:3000"

    if not server.startswith(("http://", "https://")):
        server = "http://" + server

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

    # Gesamtgröße der hochzuladenden Dateien berechnen
    upload_sizes = {}
    for rel in upload_paths:
        p = vault / Path(rel)
        try:
            upload_sizes[rel] = p.stat().st_size
        except OSError:
            upload_sizes[rel] = 0
    total_bytes = sum(upload_sizes.values())
    total_ops = len(upload_paths) + len(delete_paths)

    # Dateien hochladen
    uploaded, errors = 0, 0
    done_bytes = 0
    done_ops = 0
    if upload_paths:
        print()
    for rel in upload_paths:
        local_path = vault / Path(rel)
        print_progress(done_ops, total_ops, done_bytes, total_bytes, rel, errors)
        ok = upload_file(server, token, rel, local_path)
        done_ops += 1
        done_bytes += upload_sizes.get(rel, 0)
        if ok:
            uploaded += 1
        else:
            errors += 1
            # Fehler auf separater Zeile ausgeben ohne den Balken zu zerstören
            sys.stdout.write(f"\n  {_colored('FEHLER', ROT)} {rel}\n")
            sys.stdout.flush()
    if upload_paths:
        print_progress(done_ops, total_ops, done_bytes, total_bytes, "", errors)

    # Dateien löschen
    deleted_count = 0
    for rel in delete_paths:
        print_progress(done_ops, total_ops, done_bytes, total_bytes, f"[löschen] {rel}", errors)
        if delete_remote(server, token, rel):
            deleted_count += 1
        else:
            errors += 1
            sys.stdout.write(f"\n  {_colored('FEHLER', ROT)} löschen: {rel}\n")
            sys.stdout.flush()
        done_ops += 1
    if total_ops:
        print_progress(total_ops, total_ops, total_bytes, total_bytes, "", errors)
        print()  # Zeilenumbruch nach dem Balken

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
