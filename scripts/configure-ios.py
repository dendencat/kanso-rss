import json
import os
from pathlib import Path
team=os.environ["APPLE_TEAM_ID"]
if len(team)!=10 or not team.isalnum(): raise SystemExit("Invalid Apple team ID")
root=Path(__file__).resolve().parents[1]
config=root/"apps/native/tauri.ios.conf.json"
config.write_text(json.dumps({"bundle":{"iOS":{"developmentTeam":team}}},indent=2)+"\n")
