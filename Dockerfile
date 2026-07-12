# syntax=docker/dockerfile:1

FROM node:24-bookworm-slim AS web-builder
WORKDIR /workspace

RUN corepack enable

COPY . .
RUN pnpm install --frozen-lockfile
RUN pnpm build:web

FROM rust:1-bookworm AS rust-builder
WORKDIR /workspace

COPY . .
COPY --from=web-builder /workspace/apps/web/dist ./apps/web/dist

ENV CARGO_TARGET_DIR=/workspace/target
RUN cargo build --release \
    --manifest-path apps/desktop/src-tauri/Cargo.toml \
    --no-default-features \
    --features server \
    --bin mcp-link-server

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin mcp-link \
    && chown -R mcp-link:mcp-link /app

COPY --from=rust-builder --chown=mcp-link:mcp-link \
    /workspace/target/release/mcp-link-server \
    /app/mcp-link-server

USER mcp-link
ENV MCP_LINK_HTTP_ADDR=0.0.0.0:3284
EXPOSE 3284

ENTRYPOINT ["/app/mcp-link-server"]
