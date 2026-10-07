"""Package the public Web shell and complete third-party notices."""
from pathlib import Path
import shutil

root = Path(__file__).resolve().parents[1]
dest = root / "dist/web"
dest.mkdir(parents=True, exist_ok=True)
for name in ["index.html", "app.js", "style.css", "sw.js", "manifest.webmanifest", "icon.svg", "licenses.html", "third-party.html"]:
    shutil.copy2(root / "web" / name, dest / name)
for name in ["LICENSE", "THIRD_PARTY_NOTICES.md"]:
    shutil.copy2(root / name, dest / name)
shutil.copytree(root / "licenses", dest / "licenses", dirs_exist_ok=True)
print(dest)
