# One Dockerfile for every Rust image (iskworks-api, iskworks-worker,
# iskworks-sde-import); pick one with `--target`. The shared builder stage
# compiles all binaries in a single cargo invocation, so building the three
# images back to back compiles the workspace once — the later targets reuse
# the builder layer.
#
# Build context is the repository root: sqlx::migrate!("../../migrations")
# embeds migrations relative to each crate at compile time, and the workspace
# members are path dependencies, so the whole workspace is needed.
FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
COPY migrations ./migrations

# Cache mounts persist the cargo registry and target dir in the build host's
# BuildKit cache across builds, so a source change recompiles only the
# workspace crates, not every dependency. They never end up in an image —
# hence copying the binaries out to /out within the same RUN.
#
# iskworks-admin ships alongside the API: it is the operator CLI for
# invite-only onboarding (create / list / disable invite codes) and shares
# the API's DATABASE_URL and embedded migrations.
RUN --mount=type=cache,id=iskworks-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=iskworks-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=iskworks-cargo-target,target=/app/target,sharing=locked \
    cargo build --release --locked \
      -p iskworks-api -p iskworks-admin -p iskworks-worker -p iskworks-sde-import \
    && mkdir -p /out \
    && cp target/release/iskworks-api target/release/iskworks-admin \
          target/release/iskworks-worker target/release/iskworks-sde-import /out/

FROM debian:bookworm-slim AS iskworks-api

# Set via --build-arg in CI (e.g. "v1.2.3 (a1b2c3d)"); defaults to "dev" for
# local `docker build` and the CI sanity-build that doesn't pass it.
ARG APP_VERSION=dev

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin iskworks

COPY --from=builder /out/iskworks-api /usr/local/bin/iskworks-api
COPY --from=builder /out/iskworks-admin /usr/local/bin/iskworks-admin

USER iskworks
ENV APP_VERSION=$APP_VERSION
ENV ISKWORKS_API_ADDR=0.0.0.0:8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
    CMD curl --fail http://127.0.0.1:8080/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/iskworks-api"]

FROM debian:bookworm-slim AS iskworks-worker

ARG APP_VERSION=dev

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin iskworks

COPY --from=builder /out/iskworks-worker /usr/local/bin/iskworks-worker

USER iskworks
ENV APP_VERSION=$APP_VERSION

ENTRYPOINT ["/usr/local/bin/iskworks-worker"]

FROM debian:bookworm-slim AS iskworks-sde-import

RUN useradd --system --create-home --shell /usr/sbin/nologin iskworks

COPY --from=builder /out/iskworks-sde-import /usr/local/bin/iskworks-sde-import

USER iskworks

# One-shot CLI, not a service: pass --path /path/to/sde.zip and DATABASE_URL
# at `docker run` time. See deploy/README.md.
ENTRYPOINT ["/usr/local/bin/iskworks-sde-import"]
