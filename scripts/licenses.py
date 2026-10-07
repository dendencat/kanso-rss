"""Generate a versioned Rust/npm inventory with upstream license texts.

Requires cargo fetch --locked and npm ci. No external Python packages.
Unknown licenses or missing license texts fail the release instead of silently
dropping notices. Cargo metadata inventories all platforms, including iOS.
"""
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
output = root / "licenses"
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked", "--offline"], cwd=root))
rows = []
missing = []
records = []
upstream_path = output / "upstream/manifest.json"
upstream = json.loads(upstream_path.read_text()) if upstream_path.exists() else {}

def collect(ecosystem, name, version, license_name, directory, fallback=None):
    if not license_name or license_name == "UNLICENSED":
        missing.append(f"{ecosystem}:{name}@{version}: no license declaration")
    safe = re.sub(r"[^a-zA-Z0-9._-]", "_", name)
    dest = output / ecosystem / f"{safe}-{version}"
    dest.mkdir(parents=True, exist_ok=True)
    candidates = []
    for file in directory.iterdir():
        if file.is_file() and re.match(r"^(licen[cs]e|copying|copyright|notice)([._-]|$)", file.name, re.I):
            candidates.append(file)
    if name == "r-efi" and (directory / "AUTHORS").is_file(): candidates.append(directory / "AUTHORS")
    if not candidates and fallback:
        candidates = [f for f in fallback.iterdir() if f.is_file() and re.match(r"^(licen[cs]e|copying|notice)([._-]|$)", f.name, re.I)]
    if not candidates:
        for record in upstream.get(f"{name}@{version}",{}).get("files",[]):
            file = root / record["path"]
            if not file.resolve().is_relative_to(output.resolve()): raise SystemExit("Invalid upstream notice path")
            if hashlib.sha256(file.read_bytes()).hexdigest() != record["sha256"]: raise SystemExit("Upstream license hash mismatch")
            candidates.append(file)
    if not candidates:
        missing.append(f"{ecosystem}:{name}@{version}: upstream license text missing")
    files = []
    for source in sorted(candidates):
        target = dest / source.name
        shutil.copy2(source, target)
        files.append({"path":target.relative_to(root).as_posix(),"sha256":hashlib.sha256(target.read_bytes()).hexdigest()})
    # SQLite is public domain and ships an embedded copyright blessing.
    if name == "libsqlite3-sys":
        amalgamation = directory / "sqlite3/sqlite3.c"
        if amalgamation.exists():
            text = amalgamation.read_text().split("*/", 1)[0] + "*/\n"
            (dest / "SQLITE-PUBLIC-DOMAIN.txt").write_text(text)
    link = dest.relative_to(root).as_posix()
    rows.append(f"| {ecosystem} | `{name}` | {version} | {license_name or 'UNKNOWN'} | [全文]({link}/) |")
    records.append({"ecosystem":ecosystem,"name":name,"version":version,"license":license_name,"files":files,
        "source_url":f"https://crates.io/api/v1/crates/{name}/{version}/download" if ecosystem=="rust" else f"https://www.npmjs.com/package/{name}/v/{version}"})

for pkg in sorted(metadata["packages"], key=lambda p:(p["name"],p["version"])):
    if pkg.get("source"):
        directory = Path(pkg["manifest_path"]).parent
        if pkg.get("license_file"):
            source = directory / pkg["license_file"]
            if source.is_file() and source.parent != directory:
                # Collect explicit license files in addition to the usual root files.
                dest = output / "rust" / f'{pkg["name"]}-{pkg["version"]}'
                dest.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, dest / source.name)
        collect("rust",pkg["name"],pkg["version"],pkg.get("license") or ("LicenseRef-Upstream" if pkg.get("license_file") else None),directory)

lock = json.loads((root / "package-lock.json").read_text())
for location,pkg in sorted(lock["packages"].items()):
    if not location: continue
    directory = root / location
    # npm optional binaries for other operating systems may not be installed.
    # They are listed in the lockfile; releases regenerate on each target host.
    if not directory.is_dir(): continue
    package_json = json.loads((directory / "package.json").read_text())
    name = package_json["name"]
    fallback = root / "node_modules/@tauri-apps/cli" if name.startswith("@tauri-apps/cli-") else None
    collect("npm",name,pkg["version"],package_json.get("license") or pkg.get("license"),directory,fallback)

(root / "THIRD_PARTY_NOTICES.md").write_text("# Third-party notices\n\nKanso RSS uses the following third-party packages. This inventory includes Rust dependencies for all supported targets and installed npm build/test tools. Original copyright and license texts are bundled under `licenses/`. Regenerate on each release host with `python3 scripts/licenses.py`. Uninstalled optional npm binaries are governed by the same upstream Tauri CLI notices.\n\nSQLite is public domain; its upstream blessing is included with libsqlite3-sys. Rust and WebView system runtimes have their own platform licenses. Feed articles remain the property of their respective rights holders.\n\n| Ecosystem | Package | Version | SPDX license | Notices |\n|---|---|---|---|---|\n" + "\n".join(rows) + "\n")
(output / "inventory.json").write_text(json.dumps(records,ensure_ascii=False,indent=2) + "\n")
if missing:
    raise SystemExit("License generation failed:\n" + "\n".join(missing))
page = ['<!doctype html><html lang="ja"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Third-party licenses — Kanso RSS</title><link rel="stylesheet" href="style.css"></head><body><main class="article-detail"><a href="licenses.html">← ライセンス</a><h2>Third-party licenses</h2><p>配布物の依存パッケージ、ライセンス全文、著作権表示とソースの案内です。</p>']
for record in records:
    page.append(f'<details><summary>{html.escape(record["name"])} {html.escape(record["version"])} — {html.escape(record["license"] or "UNKNOWN")}</summary><p><a href="{html.escape(record["source_url"],quote=True)}" rel="noopener noreferrer">ソースとパッケージ</a></p>')
    for file in record['files']:
        content=(root/file['path']).read_text(errors='replace')
        page.append(f'<h3>{html.escape(Path(file["path"]).name)}</h3><pre class="license-text">{html.escape(content)}</pre>')
    page.append('</details>')
page.append('</main></body></html>')
(root/'web/third-party.html').write_text('\n'.join(page)+'\n')
print(f"Generated notices for {len(records)} dependencies")
