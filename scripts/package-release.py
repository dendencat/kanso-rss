import shutil
import sys
import subprocess
import zipfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
target=sys.argv[1]
if not target or not all(c.isalnum() or c in '-_' for c in target): raise SystemExit("Invalid target")
binaries=root/"target"/target/"release"
if len(sys.argv)>2 and sys.argv[2]=="--host-build":
    info=subprocess.check_output(["rustc","-vV"],text=True)
    host=next(line.removeprefix("host: ") for line in info.splitlines() if line.startswith("host: "))
    if host!=target: raise SystemExit("Host compiler target does not match requested package")
    binaries=root/"target/release"
dest=root/"dist"/f"kanso-{target}"
dest.mkdir(parents=True,exist_ok=True)
suffix=".exe" if "windows" in target else ""
for binary in ["kanso","kanso-server"]:
    shutil.copy2(binaries/(binary+suffix),dest)
for file in ["LICENSE","THIRD_PARTY_NOTICES.md","README.md"]: shutil.copy2(root/file,dest)
shutil.copytree(root/"licenses",dest/"licenses",dirs_exist_ok=True)
shutil.copytree(root/"docs",dest/"docs",dirs_exist_ok=True)
shutil.copytree(root/"scripts",dest/"scripts",dirs_exist_ok=True,ignore=shutil.ignore_patterns('__pycache__','*.pyc'))
with zipfile.ZipFile(str(dest)+".zip","w",zipfile.ZIP_DEFLATED,compresslevel=9,strict_timestamps=False) as archive:
    for file in sorted(dest.rglob('*')):
        if file.is_file(): archive.write(file,file.relative_to(dest))
