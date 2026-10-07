"""Verify release ZIP integrity and the exact license texts being distributed."""
import hashlib
import json
import sys
import zipfile
from pathlib import Path

directory = Path(sys.argv[1])
archives = sorted(directory.glob("*.zip"))
if not archives:
    raise SystemExit("No release ZIPs found")
for file in archives:
    with zipfile.ZipFile(file) as archive:
        damaged = archive.testzip()
        if damaged:
            raise SystemExit(f"{file.name}: corrupt ZIP entry {damaged}")
        names = set(archive.namelist())
        prefix = "web/" if "web/LICENSE" in names else ""
        for required in ("LICENSE", "THIRD_PARTY_NOTICES.md", "licenses/inventory.json"):
            if prefix + required not in names:
                raise SystemExit(f"{file.name}: missing {required}")
        inventory = json.loads(archive.read(prefix + "licenses/inventory.json"))
        for dependency in inventory:
            for notice in dependency["files"]:
                if hashlib.sha256(archive.read(prefix + notice["path"])).hexdigest() != notice["sha256"]:
                    raise SystemExit(f"{file.name}: license hash mismatch {notice['path']}")
        if "-native-" in file.name and prefix + "UNSIGNED.txt" not in names:
            raise SystemExit(f"{file.name}: missing unsigned build notice")
    print(f"{file.name}: ZIP CRC and {len(inventory)} dependency notice records verified")
