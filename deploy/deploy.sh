#!/usr/bin/env bash
# Install at /opt/kanso/deploy.sh; repo and .env stay on the deployment host.
set -euo pipefail
umask 077
revision=${1:?Exact Git commit is required}
[[ "$revision" =~ ^[a-f0-9]{40}$ ]] || { echo 'Invalid revision' >&2; exit 1; }
base=/opt/kanso
exec 9>"$base/deploy.lock"
flock -n 9 || { echo 'Another deployment is running' >&2; exit 1; }
git -C "$base/repo" fetch origin "$revision"
release="$base/releases/$revision"
mkdir -p "$release"
git -C "$base/repo" archive "$revision" | tar -x -C "$release"
docker build --pull -t "kanso-server:$revision" "$release"
previous=$(cat "$base/current-image" 2>/dev/null || true)
export KANSO_IMAGE="kanso-server:$revision"
compose=(docker compose --env-file "$base/.env" -f "$release/deploy/compose.yml")
if [[ -n "$previous" ]]; then
    # The running image supplies the backup command; no database files are
    # copied directly while SQLite is live.
    docker compose --env-file "$base/.env" -f "$base/current/deploy/compose.yml" exec -T reader kanso-server backup --output "/app/data/backups/pre-$revision-$(date -u +%Y%m%dT%H%M%SZ).sqlite"
fi
if ! "${compose[@]}" up -d --no-build --wait --wait-timeout 120; then
    if [[ -n "$previous" ]]; then
        export KANSO_IMAGE="$previous"
        docker compose --env-file "$base/.env" -f "$base/current/deploy/compose.yml" up -d --no-build --wait --wait-timeout 120
    fi
    echo 'Deployment failed; previous image restored when available' >&2
    exit 1
fi
printf '%s\n' "$KANSO_IMAGE" > "$base/current-image"
ln -sfn "$release" "$base/current"
echo "Deployed $revision"
