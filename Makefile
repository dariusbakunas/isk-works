.PHONY: db-up db-down api api-dev worker worker-dev sde-import invite invite-create web fmt clippy rust-test web-check web-test web-build verify

-include .env

DATABASE_URL ?= postgres://iskworks:iskworks@127.0.0.1:5432/iskworks
ISKWORKS_API_ADDR ?= 127.0.0.1:8080

export DATABASE_URL
export ISKWORKS_API_ADDR
export EVE_SSO_CLIENT_ID
export EVE_SSO_REDIRECT_URI
export EVE_SSO_AUTHORIZATION_URL
export EVE_SSO_TOKEN_URL
export EVE_SSO_JWKS_URL
export EVE_SSO_ISSUER
export EVE_ESI_BASE_URL
export TOKEN_ENCRYPTION_KEY
export WEB_APP_URL
export ISKWORKS_ESI_MOCK
export ISKWORKS_WORKER_MARKET_POLL_SECONDS
export ISKWORKS_WORKER_MARKET_FRESHNESS_SECONDS
export ISKWORKS_WORKER_ADJUSTED_PRICE_POLL_SECONDS
export ISKWORKS_WORKER_ADJUSTED_PRICE_FRESHNESS_SECONDS
export ISKWORKS_WORKER_SYSTEM_INDEX_POLL_SECONDS
export ISKWORKS_WORKER_SYSTEM_INDEX_FRESHNESS_SECONDS
export ISKWORKS_WORKER_LEASE_SECONDS
export ISKWORKS_WORKER_RETRY_SECONDS
export ISKWORKS_ADMIN_CHARACTER_IDS
export ISKWORKS_ENVIRONMENT_LABEL
export ISKWORKS_ESI_CONTACT

db-up:
	docker compose up -d db

db-down:
	docker compose down

api:
	cargo run -p iskworks-api

# Local/agent-verification-only: mounts POST /api/auth/dev/login, which
# bypasses the real EVE SSO round-trip. Never used by `make api` or any
# release build — see apps/iskworks-api/src/routes/dev_auth.rs.
api-dev:
	ISKWORKS_DEV_AUTH=1 cargo run -p iskworks-api --features dev-auth

worker:
	cargo run -p iskworks-worker

worker-dev: worker

sde-import:
	@test -n "$(SDE_PATH)" || (echo "SDE_PATH is required: make sde-import SDE_PATH=/path/to/eve-online-static-data-jsonl.zip" && exit 2)
	cargo run -p iskworks-sde-import -- --path "$(SDE_PATH)"

# Invite-only alpha operator tooling. Examples:
#   make invite-create ARGS="--max-uses 1 --note 'friend'"
#   make invite ARGS="list"
#   make invite ARGS="disable <uuid>"
invite:
	cargo run -p iskworks-admin -- invite $(ARGS)

invite-create:
	cargo run -p iskworks-admin -- invite create $(ARGS)

web:
	npm --prefix web/iskworks-web run dev

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

rust-test:
	cargo test --workspace

web-check:
	npm run check

web-test:
	npm test

web-build:
	npm run build

verify: fmt clippy rust-test web-check web-test web-build
