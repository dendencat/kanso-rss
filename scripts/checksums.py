import hashlib
from pathlib import Path
import sys
root=Path(sys.argv[1])
files=[p for p in sorted(root.iterdir()) if p.is_file() and p.name!="SHA256SUMS"]
(root/"SHA256SUMS").write_text("".join(f"{hashlib.file_digest(p.open('rb'),'sha256').hexdigest()}  {p.name}\n" for p in files))
