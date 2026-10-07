"""Consistent live SQLite backups; restore with the application stopped.

Examples:
  python3 scripts/backup.py backup data/kanso.sqlite backups/kanso-2026-10-07.sqlite
  python3 scripts/backup.py verify backups/kanso-2026-10-07.sqlite
  python3 scripts/backup.py restore backups/kanso-2026-10-07.sqlite data/restored.sqlite
Existing destination files are never overwritten. Stop the server before restore.
"""
import argparse
import os
from pathlib import Path
import sqlite3
import tempfile

def verified(path):
    uri = path.resolve().as_uri() + "?mode=ro"
    db = sqlite3.connect(uri,uri=True)
    if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
        db.close()
        raise ValueError("Database integrity check failed")
    if db.execute("PRAGMA user_version").fetchone()[0] != 1:
        db.close()
        raise ValueError("Unsupported database schema")
    return db

def copy(source,dest):
    dest.parent.mkdir(parents=True,exist_ok=True)
    if dest.exists(): raise FileExistsError("Destination already exists")
    source_db = verified(source)
    fd,tmp = tempfile.mkstemp(prefix=".kanso-backup-",suffix=".sqlite",dir=dest.parent)
    os.close(fd)
    try:
        with sqlite3.connect(tmp) as output:
            source_db.backup(output)
            if output.execute("PRAGMA integrity_check").fetchone()[0] != "ok": raise ValueError("Backup integrity check failed")
        # Publish without replacing an existing file, even if another backup won.
        os.link(tmp,dest)
    finally:
        source_db.close()
        Path(tmp).unlink(missing_ok=True)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command",choices=["backup","restore","verify"])
    parser.add_argument("source",type=Path)
    parser.add_argument("destination",type=Path,nargs="?")
    args=parser.parse_args()
    if args.command == "verify": verified(args.source).close()
    else:
        if not args.destination: parser.error("Destination is required")
        copy(args.source,args.destination)
    print("OK")
