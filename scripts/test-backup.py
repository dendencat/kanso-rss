"""Exercise backup/restore against a live WAL database and invalid sources."""
from pathlib import Path
import sqlite3
import tempfile
import subprocess
import sys

script=Path(__file__).with_name("backup.py")
with tempfile.TemporaryDirectory() as directory:
    root=Path(directory)
    source=root/"live.sqlite"
    live=sqlite3.connect(source)
    live.execute("PRAGMA journal_mode=WAL")
    live.execute("PRAGMA user_version=1")
    live.execute("CREATE TABLE feeds (id TEXT)")
    live.execute("INSERT INTO feeds VALUES ('durable')")
    live.commit()
    backup=root/"backup.sqlite"
    subprocess.run([sys.executable,str(script),"backup",str(source),str(backup)],check=True)
    subprocess.run([sys.executable,str(script),"verify",str(backup)],check=True)
    restored=root/"restored.sqlite"
    subprocess.run([sys.executable,str(script),"restore",str(backup),str(restored)],check=True)
    with sqlite3.connect(restored) as db: assert db.execute("SELECT id FROM feeds").fetchone()[0]=="durable"
    assert subprocess.run([sys.executable,str(script),"backup",str(source),str(backup)],capture_output=True).returncode!=0
    assert subprocess.run([sys.executable,str(script),"verify",str(root/"missing.sqlite")],capture_output=True).returncode!=0
    live.close()
print("PASS: live WAL backup, integrity check, restore and overwrite protection")
