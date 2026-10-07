"""Bounded local API load test with synthetic data in a disposable database.

Measures authenticated article lists, indexed Japanese search and stats under
concurrent requests. It is a repeatable regression check, not a capacity claim
for a deployment. No external feed traffic is generated.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import urllib.request
import uuid

root=Path(__file__).resolve().parents[1]
server=root/"target/debug"/("kanso-server.exe" if os.name=="nt" else "kanso-server")
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
def ready(url,process):
    for _ in range(100):
        if process.poll() is not None: raise RuntimeError("Server exited")
        try: opener.open(url+"/healthz",timeout=1).close(); return
        except OSError: time.sleep(.1)
    raise RuntimeError("Server did not become ready")
with tempfile.TemporaryDirectory() as directory:
    with socket.socket() as sock: sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
    url=f"http://127.0.0.1:{port}"
    token=subprocess.check_output([server,"token"],text=True).strip()
    db_path=Path(directory)/"load.sqlite"
    env={**os.environ,"KANSO_TOKEN":token,"KANSO_LISTEN":f"127.0.0.1:{port}","KANSO_DATABASE":str(db_path)}
    command=[server,"--refresh-seconds","0"]
    process=subprocess.Popen(command,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    ready(url,process);process.terminate();process.wait(timeout=10)
    with sqlite3.connect(db_path) as db:
        for f in range(100):
            feed=uuid.uuid4().hex
            db.execute("INSERT INTO feeds(id,url,title,folder) VALUES(?,?,?,?)",(feed,f"https://example.org/{f}.xml",f"Feed {f}","Test"))
            rows=[(uuid.uuid4().hex,feed,str(i),f"記事検索の安全性 {i}","<p>テスト用の記事です。検索の安全性を確認します。</p>"*20,"2026-10-07T00:00:00Z") for i in range(100)]
            db.executemany("INSERT INTO articles(id,feed_id,entry_key,title,content,published) VALUES(?,?,?,?,?,?)",rows)
    process=subprocess.Popen(command,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        ready(url,process)
        def request(index):
            path=['/api/v1/articles?limit=50','/api/v1/articles?q=%E6%A4%9C%E7%B4%A2%E3%81%AE%E5%AE%89%E5%85%A8&limit=50','/api/v1/stats'][index%3]
            req=urllib.request.Request(url+path,headers={"Authorization":"Bearer "+token})
            start=time.perf_counter()
            with opener.open(req,timeout=10) as response:
                result=json.load(response)
                if index%3<2: assert len(result)==50
                else: assert result['articles']==10000
            return (time.perf_counter()-start)*1000
        start=time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool: durations=list(pool.map(request,range(200)))
        elapsed=time.perf_counter()-start
        result={"fixture_feeds":100,"fixture_articles":10000,"requests":200,"workers":16,"failures":0,"elapsed_seconds":round(elapsed,3),"median_ms":round(statistics.median(durations),2),"p95_ms":round(sorted(durations)[int(len(durations)*.95)-1],2),"build":"debug","scope":"local synthetic API regression test"}
        artifacts=root/'artifacts';artifacts.mkdir(exist_ok=True)
        (artifacts/'load-test.json').write_text(json.dumps(result,indent=2)+'\n')
        assert result['p95_ms']<5000,"Unexpected latency regression"
        print(json.dumps(result))
    finally:
        process.terminate();process.wait(timeout=10)
