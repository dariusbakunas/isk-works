# Build context is this directory (web/iskworks-web) — the app is a
# self-contained npm package, unlike the API which needs the repo root.
FROM node:22-bookworm AS builder
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY . .

# Release label for session-replay recordings. Session replay itself is
# configured at container start, not here -- see
# docker/40-iskworks-runtime-config.sh and
# docs/security/session-replay-privacy.md.
ARG VITE_APP_RELEASE="dev"
ENV VITE_APP_RELEASE=$VITE_APP_RELEASE

RUN npm run build

FROM nginx:1.27-alpine AS runtime

# Same-origin deployment: the SPA calls /api/* on its own domain (see
# deploy/docker-compose.yml, which path-routes /api to iskworks-api on the
# same host), so no VITE_API_BASE_URL is baked in at build time.
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
# The nginx entrypoint runs executable /docker-entrypoint.d/*.sh before
# starting nginx; this one writes /config.js from LOGROCKET_* env vars.
COPY --chmod=755 docker/40-iskworks-runtime-config.sh /docker-entrypoint.d/40-iskworks-runtime-config.sh

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD wget --quiet --spider http://127.0.0.1/index.html || exit 1
