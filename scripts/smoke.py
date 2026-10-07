"""Run the actual Rust server and CLI against an isolated SQLite database."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request
import urllib.error

root=Path(__file__).resolve().parents[1]
suffix=".exe" if os.name=="nt" else ""
server=root/"target/debug"/("kanso-server"+suffix)
cli=root/"target/debug"/("kanso"+suffix)
with tempfile.TemporaryDirectory() as directory:
    with socket.socket() as sock: sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
    token=subprocess.check_output([server,"token"],text=True).strip()
    env={**os.environ,"KANSO_TOKEN":token,"KANSO_API_URL":f"http://127.0.0.1:{port}","KANSO_LISTEN":f"127.0.0.1:{port}","KANSO_DATABASE":str(Path(directory)/"db.sqlite")}
    process=subprocess.Popen([server,"--refresh-seconds","0"],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    def run(*args): return subprocess.check_output([cli,*args],env=env,text=True)
    try:
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
        for _ in range(100):
            if process.poll() is not None: raise RuntimeError(process.stderr.read().decode())
            try: opener.open(f"http://127.0.0.1:{port}/healthz",timeout=1).close(); break
            except OSError: time.sleep(.1)
        else: raise RuntimeError("Server did not start")
        try: opener.open(f"http://127.0.0.1:{port}/api/v1/feeds"); raise AssertionError("Unauthenticated API succeeded")
        except urllib.error.HTTPError as error: assert error.code==401
        feed=json.loads(run("add","https://example.org/feed.xml","--title","CLI Test","--folder","Tech"))
        assert json.loads(run("feeds"))[0]["id"]==feed["id"]
        assert json.loads(run("stats"))["feeds"]==1
        opml=run("export")
        assert "CLI Test" in opml and "Tech" in opml
        imported=Path(directory)/"import.opml"; imported.write_text(opml)
        assert json.loads(run("import",str(imported)))["imported"]==0
        backup=Path(directory)/"backup.sqlite"
        subprocess.run([server,"backup","--output",backup],env=env,check=True,stdout=subprocess.DEVNULL)
        run("remove",feed["id"])
        assert json.loads(run("stats"))["feeds"]==0
        subprocess.run([server,"healthcheck"],env=env,check=True)
        assert subprocess.run([cli,"add","http://169.254.169.254/latest/meta-data"],env=env,capture_output=True).returncode!=0
    finally:
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired: process.kill(); process.wait()
print("PASS: real Rust server/CLI authentication, subscriptions, OPML, live backup, healthcheck, deletion and SSRF rejection")
