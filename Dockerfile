# syntax=docker/dockerfile:1
# PayRouters: one image with the Rust server and the built dashboard.
# Build: docker build --build-arg VERSION=0.1.0 -t payrouters .

# --- Dashboard (SvelteKit, adapter-static) ---------------------------------------------------
FROM node:22-alpine AS web
WORKDIR /web
RUN npm install --global pnpm@10
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# --- Server ----------------------------------------------------------------------------------
FROM rust:1-bookworm AS build
WORKDIR /src
# Dependencies first, against stub sources, so this layer is cached until a manifest changes.
COPY Cargo.toml Cargo.lock ./
COPY crates/payrouters-types/Cargo.toml crates/payrouters-types/Cargo.toml
COPY crates/payrouters-sdk/Cargo.toml crates/payrouters-sdk/Cargo.toml
COPY crates/payrouters-server/Cargo.toml crates/payrouters-server/Cargo.toml
RUN mkdir -p crates/payrouters-types/src crates/payrouters-sdk/src crates/payrouters-server/src \
    && touch crates/payrouters-types/src/lib.rs crates/payrouters-sdk/src/lib.rs crates/payrouters-server/src/lib.rs \
    && echo 'fn main() {}' > crates/payrouters-server/src/main.rs \
    && cargo build --release --locked -p payrouters-server \
    && rm -rf crates/*/src
COPY crates crates
# The stub build left newer mtimes on the workspace crates; force a rebuild of them.
RUN touch crates/*/src/*.rs \
    && cargo build --release --locked -p payrouters-server \
    && cp target/release/payrouters-server /payrouters-server

# --- Runtime ---------------------------------------------------------------------------------
FROM debian:bookworm-slim
ARG VERSION=dev
LABEL org.opencontainers.image.title="PayRouters" \
      org.opencontainers.image.description="Cash-in payment router for Midtrans, Xendit and IDRX" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="https://github.com/igun997/payrouters"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home payrouters
COPY --from=build /payrouters-server /usr/local/bin/payrouters-server
COPY --from=web /web/build /app/web
ENV WEB_DIR=/app/web \
    LISTEN_ADDR=0.0.0.0:8080 \
    RUST_LOG=info
USER payrouters
EXPOSE 8080
CMD ["payrouters-server"]
