#!/usr/bin/env python3
"""
Obsidian Publisher Upload Script

Reads the vault, finds all notes with `Publiziert: true` in their frontmatter,
collects embedded assets, applies whitelist/blacklist rules, and uploads
everything to the configured FTP server.

Usage:
    python publish.py <vault_path>
    python publish.py C:/Users/me/Documents/MyVault

Environment variables (all optional, defaults shown):
    FTP_HOST     FTP server hostname / IP              (localhost)
    FTP_PORT     FTP server port                       (21)
    FTP_USER     FTP username                          (obsidian)
    FTP_PASS     FTP password                          (obsidian)

whitelist.json / blacklist.json support glob patterns relative to vault root:
    "Notes/Artikel.md"      -- exact file
    "Assets/**"             -- everything inside Assets/
    "**/*.png"              -- all PNG files anywhere in the vault
    "Private/**"            -- entire Private/ folder (blacklist)
"""

import argparse
import fnmatch
import ftplib
import json
import os
import re
import sys
import urllib.request
from pathlib import Path
from typing import Optional


def _load_dotenv(path: Path) -> None:
    """Load a .env file into os.environ (no external dependencies)."""
    if not path.exists():
        return
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip().strip('"').strip("'")
            os.environ.setdefault(key, val)


_load_dotenv(Path(__file__).parent / ".env")

# ── CONFIG (override via .env or environment variables) ───────────────────────
FTP_HOST  = os.environ.get("FTP_HOST",  "localhost")
FTP_PORT  = int(os.environ.get("FTP_PORT", "21"))
FTP_USER  = os.environ.get("FTP_USER",  "obsidian")
FTP_PASS  = os.environ.get("FTP_PASS",  "obsidian")
WEB_URL   = os.environ.get("BASE_URL",  "http://localhost:3000")

SCRIPT_DIR     = Path(__file__).parent
WHITELIST_FILE = SCRIPT_DIR / "whitelist.json"
BLACKLIST_FILE = SCRIPT_DIR / "blacklist.json"
# ──────────────────────────────────────────────────────────────────────────────

# Obsidian wiki-embed:       ![[file.png]]  or  ![[file.png|alias]]
_WIKI_EMBED_RE  = re.compile(r'!\[\[([^\]|]+)(?:\|[^\]]*)?\]\]')
# Standard markdown image:   ![alt](path)
_MD_IMAGE_RE    = re.compile(r'!\[[^\]]*\]\(([^)]+)\)')
# Obsidian wiki-link to PDF: [[doc.pdf]]  or  [[doc.pdf|alias]]
_WIKI_PDF_RE    = re.compile(r'(?<!!)\[\[([^\]|]+\.pdf)(?:\|[^\]]*)?\]\]', re.IGNORECASE)
# Markdown link to local PDF: [text](doc.pdf)
_MD_PDF_RE      = re.compile(r'(?<!!)\[[^\]]*\]\(([^)]+\.pdf)\)', re.IGNORECASE)
# YAML frontmatter block
_FRONTMATTER_RE = re.compile(r'^---[ \t]*\r?\n(.*?)\r?\n---[ \t]*\r?\n', re.DOTALL)


def load_list(path: Path) -> list[str]:
    """Load a JSON array of strings from a file; return [] if missing."""
    if not path.exists():
        return []
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    if not isinstance(data, list):
        print(f"[warn] {path.name} is not a JSON array – ignored")
        return []
    return [str(e).replace("\\", "/") for e in data]


def path_matches(rel_str: str, pattern: str) -> bool:
    """
    Test whether a vault-relative path matches a glob pattern.

    Supports:
        exact/path/file.md    – literal match
        folder/**             – everything inside folder/ (any depth)
        **/*.png              – all .png files anywhere
        *.md                  – .md files in vault root only
    """
    rel = rel_str.replace("\\", "/")
    pat = pattern.replace("\\", "/")

    if "**" not in pat:
        return fnmatch.fnmatch(rel, pat)

    # Convert ** glob to a regex so fnmatch semantics extend across /
    import re as _re
    parts = pat.split("**")
    regex = ".*".join(_re.escape(p).replace(r"\*", "[^/]*") for p in parts)
    return bool(_re.fullmatch(regex, rel))


def expand_whitelist(vault: Path, patterns: list[str]) -> list[Path]:
    """Expand whitelist patterns to actual file paths using vault glob."""
    result: list[Path] = []
    for pattern in patterns:
        matches = [p for p in vault.glob(pattern) if p.is_file()]
        if not matches:
            print(f"  [warn/whitelist]  no files matched: {pattern}")
        result.extend(matches)
    return result


def is_published(content: str) -> bool:
    """Return True when the frontmatter contains `Publiziert: true`."""
    m = _FRONTMATTER_RE.match(content)
    if not m:
        return False
    for line in m.group(1).splitlines():
        if ":" not in line:
            continue
        key, _, val = line.partition(":")
        if key.strip().lower() == "publiziert":
            return val.strip().lower() in ("true", "yes", "1", "ja")
    return False


def find_asset(name: str, vault: Path) -> Optional[Path]:
    """
    Locate an embedded asset inside the vault.
    Tries the exact relative path first, then a vault-wide name search.
    """
    # Strip any query string / anchor that editors sometimes append
    name = name.split("?")[0].split("#")[0].strip()

    direct = vault / name
    if direct.exists():
        return direct

    filename = Path(name).name
    for p in vault.rglob(filename):
        # Skip hidden directories (.obsidian, .git, …)
        if any(part.startswith(".") for part in p.relative_to(vault).parts):
            continue
        return p
    return None


def embedded_assets(content: str, vault: Path) -> list[Path]:
    """Collect all locally embedded/linked asset paths mentioned in *content*."""
    found: list[Path] = []

    def _add(target: str) -> None:
        target = target.strip()
        if target.startswith(("http://", "https://", "data:")):
            return
        asset = find_asset(target, vault)
        if asset:
            found.append(asset)

    # ![[image.png]] / ![[doc.pdf]] — Obsidian embeds (images, PDFs, …)
    for m in _WIKI_EMBED_RE.finditer(content):
        _add(m.group(1).split("|")[0])

    # ![alt](image.png) / ![](doc.pdf) — markdown embeds
    for m in _MD_IMAGE_RE.finditer(content):
        _add(m.group(1))

    # [[doc.pdf]] — Obsidian wiki-link to PDF (non-embed)
    for m in _WIKI_PDF_RE.finditer(content):
        _add(m.group(1).split("|")[0])

    # [Text](doc.pdf) — markdown link to local PDF
    for m in _MD_PDF_RE.finditer(content):
        _add(m.group(1))

    return found


def collect(vault: Path, whitelist: list[str], blacklist: list[str]) -> list[Path]:
    """
    Walk the vault, pick up published notes + their assets,
    apply whitelist / blacklist, return a de-duplicated list.
    """
    vault_abs = vault.resolve()
    upload:   list[Path] = []
    seen:     set[Path]  = set()

    def is_blacklisted(rel_str: str) -> bool:
        return any(path_matches(rel_str, pat) for pat in blacklist)

    def enqueue(p: Path, reason: str = "") -> bool:
        p = p.resolve()
        if p in seen:
            return False
        try:
            rel = p.relative_to(vault_abs)
        except ValueError:
            print(f"  [skip/outside-vault] {p}")
            return False
        rel_str = str(rel).replace("\\", "/")
        if is_blacklisted(rel_str):
            print(f"  [skip/blacklist]  {rel_str}")
            return False
        seen.add(p)
        upload.append(p)
        tag = f"[{reason}]" if reason else ""
        print(f"  {tag:<18} {rel_str}")
        return True

    print("Scanning vault for published notes …\n")

    for md in sorted(vault.rglob("*.md")):
        # Skip hidden dirs (.obsidian, .trash, …)
        if any(part.startswith(".") for part in md.relative_to(vault).parts):
            continue
        content = md.read_text(encoding="utf-8", errors="replace")
        if not is_published(content):
            continue
        if enqueue(md, "published"):
            for asset in embedded_assets(content, vault):
                enqueue(asset, "asset")

    # Whitelist: force-include regardless of Publiziert flag
    for p in expand_whitelist(vault, whitelist):
        enqueue(p, "whitelist")

    return upload


# ── FTP helpers ────────────────────────────────────────────────────────────────

def ftp_mkdirs(ftp: ftplib.FTP, remote_dir: str) -> None:
    """Recursively create directories on the FTP server (silently skip existing)."""
    parts = [p for p in remote_dir.replace("\\", "/").split("/") if p and p != "."]
    current = ""
    for part in parts:
        current = f"{current}/{part}" if current else part
        try:
            ftp.mkd(current)
        except ftplib.error_perm:
            pass  # already exists


def upload_files(files: list[Path], vault: Path) -> None:
    vault_abs = vault.resolve()

    print(f"\nConnecting to ftp://{FTP_HOST}:{FTP_PORT}  (user: {FTP_USER}) …")
    with ftplib.FTP() as ftp:
        ftp.connect(FTP_HOST, FTP_PORT, timeout=30)
        ftp.login(FTP_USER, FTP_PASS)
        ftp.set_pasv(True)
        print(f"Connected. Uploading {len(files)} file(s) …\n")

        for local in files:
            rel      = local.resolve().relative_to(vault_abs)
            remote   = str(rel).replace("\\", "/")
            parent   = str(rel.parent).replace("\\", "/")

            if parent and parent != ".":
                ftp_mkdirs(ftp, parent)

            with open(local, "rb") as f:
                ftp.storbinary(f"STOR {remote}", f)

            size = local.stat().st_size
            print(f"  ✓  {remote}  ({size:,} bytes)")


# ── Entry point ────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Upload published Obsidian notes to the FTP server."
    )
    parser.add_argument(
        "vault_path",
        help="Path to the Obsidian vault directory",
    )
    return parser.parse_args()


def trigger_reload() -> None:
    """Call /api/reload on the web server so new files become visible."""
    url = f"{WEB_URL.rstrip('/')}/api/reload"
    try:
        with urllib.request.urlopen(url, timeout=10) as resp:
            body = resp.read().decode()
            print(f"  Server reload: {body.strip()}")
    except Exception as exc:
        print(f"  [warn] reload failed ({url}): {exc}")


def main() -> None:
    args  = parse_args()
    vault = Path(args.vault_path).resolve()

    if not vault.exists():
        print(f"Error: vault path does not exist: {vault}", file=sys.stderr)
        sys.exit(1)

    whitelist = load_list(WHITELIST_FILE)
    blacklist = load_list(BLACKLIST_FILE)

    print(f"Vault      : {vault}")
    print(f"FTP target : {FTP_HOST}:{FTP_PORT}")
    print(f"Web server : {WEB_URL}")
    print(f"Whitelist  : {len(whitelist)} pattern(s)")
    print(f"Blacklist  : {len(blacklist)} pattern(s)\n")

    files = collect(vault, whitelist, blacklist)

    if not files:
        print("\nNothing to upload.")
        return

    print(f"\n{len(files)} file(s) queued.")
    upload_files(files, vault)

    print("\nTriggering server reload …")
    trigger_reload()
    print(f"\nDone – {len(files)} file(s) uploaded and indexed.")


if __name__ == "__main__":
    main()
