"""Package unsigned desktop CI bundles with complete dependency notices."""
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
source = Path(sys.argv[1])
output = Path(sys.argv[2])
output.mkdir(parents=True, exist_ok=True)
bundles = sorted(source.glob("unsigned-native-*"))
if len(bundles) != 3:
    raise SystemExit("Expected Windows and both macOS desktop bundles")
for bundle in bundles:
    if not any(path.is_file() for path in bundle.rglob("*")):
        raise SystemExit(f"Empty desktop bundle: {bundle.name}")
    name = bundle.name.replace("unsigned-native-", "kanso-native-")
    with tempfile.TemporaryDirectory() as temporary:
        package = Path(temporary) / name
        shutil.copytree(bundle, package / "bundle")
        for file in ("LICENSE", "THIRD_PARTY_NOTICES.md", "README.md"):
            shutil.copy2(root / file, package / file)
        shutil.copytree(root / "licenses", package / "licenses")
        shutil.copytree(root / "docs", package / "docs")
        (package / "UNSIGNED.txt").write_text(
            "These desktop application bundles are unsigned. Windows signing and "
            "macOS notarization require the platform signing workflows and certificates.\n"
        )
        destination = output / f"{name}-unsigned.zip"
        with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9,
                             strict_timestamps=False) as archive:
            for file in sorted(package.rglob("*")):
                if file.is_file():
                    archive.write(file, file.relative_to(package))
        print(destination.name)
