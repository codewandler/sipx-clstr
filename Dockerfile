# The node image.
#
# The binary is built *inside* the builder stage rather than copied from the host: a host-built
# binary links the host's glibc, and a development machine's glibc is routinely newer than the
# runtime image's, so the copy would produce an image that runs nowhere but the machine that built
# it. Building here costs a cold compile once and is then cached.
#
# Only `sipx-clstr-node` is built. The other crates are libraries the workspace resolves as
# dependencies; asking for the whole workspace would build the simulator and the test harness into
# an image that never runs them.

# The workspace's `rust-version`, which `CF-9` made true: the floor was 1.88, the workspace did
# not build on it, and this image pinned 1.97 as a stopgap. The floor is now 1.94 — measured, and
# re-measured on every gate run and every CI run by `scripts/check-msrv.sh`.
#
# Building the image on the declared floor rather than on current stable is deliberate. It is what
# caught the original defect, and it keeps catching it: a development machine runs stable and
# stable builds everything, so this pin is one of the few places the claim is actually exercised.
# Keep it equal to `rust-version` in Cargo.toml.
ARG RUST_VERSION=1.94

# ---------------------------------------------------------------------------------- builder ---
FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /src

# `--all-features` is deliberately NOT used: the `postgres` feature pulls a database driver into a
# node that, in the devspace profile, uses the in-memory location store. KO-2's chart is where the
# PostgreSQL-backed variant belongs.
ARG CARGO_PROFILE=dev

COPY Cargo.toml Cargo.lock clippy.toml ./
COPY crates/ crates/

# Cache mounts keep the registry and the target directory out of the image layers, so an iteration
# recompiles only what changed. The binary is copied to /out inside the same RUN because a cache
# mount is not part of the resulting layer — anything left in it is gone when the step ends.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    set -eux; \
    if [ "$CARGO_PROFILE" = "release" ]; then \
        cargo build --release --locked -p sipx-clstr-node --bin sipx-clstr; \
        install -D /src/target/release/sipx-clstr /out/sipx-clstr; \
    else \
        cargo build --locked -p sipx-clstr-node --bin sipx-clstr; \
        install -D /src/target/debug/sipx-clstr /out/sipx-clstr; \
    fi; \
    strip /out/sipx-clstr || true

# ---------------------------------------------------------------------------------- runtime ---
FROM debian:bookworm-slim AS runtime

# ca-certificates is present for TLS listeners (DP-5 adds them); tini reaps the process tree so a
# `devspace dev` restart does not leave a node holding the port.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

# An unprivileged user. The node binds 5060, which is above 1024, so no capability is needed —
# a SIP proxy that required root to answer a port would be a deployment constraint invented here
# rather than one the protocol asks for.
RUN useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin sipx
USER 10001:10001

COPY --from=builder /out/sipx-clstr /usr/local/bin/sipx-clstr

# The driver logs to stderr and reads its filter from RUST_LOG (see `run_node`).
ENV RUST_LOG=info

EXPOSE 5060/udp 5060/tcp

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/sipx-clstr"]

# Help, not `run`. Since DP-5 a node refuses to start when it would advertise an unspecified
# address, so `run --listen 0.0.0.0:5060` — the only default a container image could reasonably
# pick — now exits 2. A default that always fails teaches nothing; the usage text names the flag
# that is missing. Every real invocation passes `--advertise`, as the manifests do.
CMD ["--help"]
