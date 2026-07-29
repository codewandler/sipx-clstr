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

# NOT the workspace's `rust-version = "1.88"`. That floor is wrong today: on 1.88 the kernel's
# `sipx-transport` fails to compile, because `BinaryHeap::<T>::new`'s `T: Ord` bound was only
# relaxed in a later rustc —
#
#   error[E0277]: the trait bound `I: Ord` is not satisfied
#   note: required for `timers::Entry<K, I>` to implement `Ord`
#   note: required by a bound in `BinaryHeap::<T>::new`
#
# A development machine on current stable never sees it, which is exactly why an image pinned to
# the declared floor is what caught it. Filed as its own story; this pin is the workaround, not
# the fix, and it comes back to the declared floor once that story settles.
ARG RUST_VERSION=1.97

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
