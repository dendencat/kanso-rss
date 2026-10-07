"""Fetch omitted crate notices from exact upstream Git commits.

Run intentionally after dependency changes. Network access is confined to the
GitHub API and raw.githubusercontent.com. Source URLs, revisions and hashes are
retained. Normal CI/release license generation uses the checked-in copies.
"""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import urllib.request

root=Path(__file__).resolve().parents[1]
metadata=json.loads(subprocess.check_output(["cargo","metadata","--locked","--offline","--format-version","1"],cwd=root))
groups={}
manifest_path=root/"licenses/upstream/manifest.json"
manifest=json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
pattern=re.compile(r"^(licen[cs]e|copying|copyright|notice)([._-]|$)",re.I)
for package in metadata["packages"]:
    if not package.get("source"): continue
    directory=Path(package["manifest_path"]).parent
    if package["name"] == "r-efi" and (directory/"AUTHORS").is_file(): continue
    if any(p.is_file() and pattern.match(p.name) for p in directory.iterdir()): continue
    repo=re.fullmatch(r"https://github.com/([^/]+/[^/]+?)(?:\.git)?/?",package.get("repository") or "")
    vcs=directory/".cargo_vcs_info.json"
    if not repo or not vcs.is_file(): continue
    info=json.loads(vcs.read_text())
    key=f"{package['name']}@{package['version']}"
    if key in manifest and all((root/f['path']).is_file() for f in manifest[key]['files']): continue
    sha=info["git"]["sha1"]
    if not re.fullmatch("[a-f0-9]{40}",sha): continue
    groups.setdefault((repo.group(1),sha),[]).append((package,info.get("path_in_vcs","")))

def download(url):
    request=urllib.request.Request(url,headers={"User-Agent":"Kanso-license-inventory","Accept":"application/vnd.github+json"})
    with urllib.request.urlopen(request,timeout=40) as response:
        data=response.read(2*1024*1024+1)
        if len(data)>2*1024*1024: raise ValueError("License/tree response too large")
        return data

def fetch(group):
    (repo,sha),packages=group
    tree=json.loads(download(f"https://api.github.com/repos/{repo}/git/trees/{sha}?recursive=1"))
    if tree.get("truncated"): raise ValueError(f"Truncated Git tree: {repo}")
    paths=[item["path"] for item in tree["tree"] if item["type"]=="blob" and pattern.match(PurePosixPath(item["path"]).name)]
    result={}
    for package,source_dir in packages:
        ancestors=[PurePosixPath(source_dir),*PurePosixPath(source_dir).parents]
        selected=[]
        for ancestor in ancestors:
            selected=[p for p in paths if PurePosixPath(p).parent==ancestor]
            if selected: break
        if not selected:
            selected=[p for p in paths if p.startswith("LICENSES/") or PurePosixPath(p).parent==PurePosixPath(".")]
        if not selected: raise ValueError(f"Cannot locate exact upstream notices: {package['name']}")
        key=f"{package['name']}@{package['version']}"
        dest=root/"licenses/upstream"/key.replace('@','-')
        dest.mkdir(parents=True,exist_ok=True)
        records=[]
        for path in selected:
            url=f"https://raw.githubusercontent.com/{repo}/{sha}/{path}"
            data=download(url)
            name=path.replace('/','__')
            target=dest/name
            target.write_bytes(data)
            records.append({"url":url,"path":target.relative_to(root).as_posix(),"sha256":hashlib.sha256(data).hexdigest()})
        result[key]={"repository":repo,"revision":sha,"files":records}
    return result

errors=[]
with ThreadPoolExecutor(max_workers=6) as pool:
    futures=[pool.submit(fetch,group) for group in groups.items()]
    for future in futures:
        try:
            records=future.result();manifest.update(records);print("Fetched",", ".join(records))
        except Exception as error: errors.append(str(error))
manifest_path.parent.mkdir(parents=True,exist_ok=True)
manifest_path.write_text(json.dumps(manifest,indent=2,sort_keys=True)+"\n")
if errors: raise SystemExit("\n".join(errors))
