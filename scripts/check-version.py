import json
import os
from pathlib import Path
import tomllib
root=Path(__file__).resolve().parents[1]
version=tomllib.loads((root/"Cargo.toml").read_text())["workspace"]["package"]["version"]
if json.loads((root/"package.json").read_text())["version"]!=version: raise SystemExit("package.json version differs from Cargo")
if json.loads((root/"apps/native/tauri.conf.json").read_text())["version"]!=version: raise SystemExit("Tauri version differs from Cargo")
if f"kanso-shell-{version}" not in (root/"web/sw.js").read_text(): raise SystemExit("Service worker cache version differs from Cargo")
ref=os.environ.get("GITHUB_REF","")
if ref.startswith("refs/tags/") and ref!=f"refs/tags/v{version}": raise SystemExit("Release tag and application version differ")
print(f"Version {version} is consistent")
