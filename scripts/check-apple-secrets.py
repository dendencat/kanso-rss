import os
required=["APPLE_CERTIFICATE","APPLE_CERTIFICATE_PASSWORD","APPLE_TEAM_ID"]
if os.environ.get("RELEASE_PLATFORM")=="ios": required += ["APPLE_PROVISIONING_PROFILE"]
else: required += ["APPLE_SIGNING_IDENTITY","APPLE_ID","APPLE_PASSWORD"]
missing=[name for name in required if not os.environ.get(name)]
if missing: raise SystemExit("Missing signing configuration: "+", ".join(missing))
print("Apple signing configuration is present")
