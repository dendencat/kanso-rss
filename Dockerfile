# syntax=docker/dockerfile:1
FROM node:24-bookworm-slim AS node-tools
FROM rust:1.90.0-slim-bookworm AS build
COPY --from=node-tools /usr/local/bin/node /usr/local/bin/node
COPY --from=node-tools /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm && apt-get update && apt-get install -y --no-install-recommends build-essential pkg-config ca-certificates python3 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/proxy_ca NODE_EXTRA_CA_CERTS=/run/secrets/proxy_ca SSL_CERT_FILE=/run/secrets/proxy_ca; fi; \
    cargo fetch --locked && npm ci --ignore-scripts && python3 scripts/licenses.py
RUN cargo build --release --locked -p kanso-server -p kanso-cli

FROM build AS native-check
RUN apt-get update && apt-get install -y --no-install-recommends libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev && rm -rf /var/lib/apt/lists/*
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/proxy_ca; fi; \
    cargo check --locked -p kanso-native

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && groupadd -g 10001 kanso && useradd -r -u 10001 -g kanso kanso && mkdir -p /app/data/backups && chown -R kanso:kanso /app
WORKDIR /app
COPY --from=build /build/target/release/kanso-server /build/target/release/kanso /usr/local/bin/
COPY --from=build --chown=10001:10001 /build/LICENSE /build/THIRD_PARTY_NOTICES.md /app/
COPY --from=build --chown=10001:10001 /build/licenses /app/licenses
USER 10001:10001
ENV KANSO_LISTEN=0.0.0.0:8080 KANSO_DATABASE=/app/data/kanso.sqlite KANSO_ALLOW_REMOTE_HTTP=true
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD ["kanso-server", "healthcheck"]
ENTRYPOINT ["kanso-server"]
