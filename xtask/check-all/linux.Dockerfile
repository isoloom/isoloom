# The linux-docker host of `cargo xtask check-all`: the pinned Rust toolchain (rust-toolchain.toml's,
# with its components and targets) with cargo-machete (the launcher's `pnpm quality` runs it), Node 24
# with pnpm (the version the launcher's packageManager names, through corepack), and the system
# packages the launcher's Tauri build needs. check-all builds it once per version of this file.
ARG RUST_VERSION=1.99.0
ARG NODE_VERSION=24.21.0

FROM node:${NODE_VERSION}-trixie-slim AS node

FROM rust:${RUST_VERSION}-slim-trixie
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential pkg-config git ca-certificates curl file \
      libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf \
 && rm -rf /var/lib/apt/lists/* \
 && rustup component add rustfmt clippy \
 && rustup target add x86_64-pc-windows-msvc \
 && cargo install --locked cargo-machete@0.9.2 \
 && rm -rf /usr/local/cargo/registry
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -s ../lib/node_modules/corepack/dist/corepack.js /usr/local/bin/corepack \
 && corepack enable pnpm
