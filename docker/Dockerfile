# cargo-chef splits the build so dependencies compile in their own layer, which the CD layer cache reuses
# until a manifest or Cargo.lock changes.
FROM lukemathwalker/cargo-chef:0.1.78-rust-1.99-slim-trixie AS chef

WORKDIR /build


FROM chef AS planner

COPY . .

# The recipe lists only manifests and the lockfile, so source-only changes leave it, and the cook layer, unchanged.
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder

COPY --from=planner /build/recipe.json recipe.json

# Builds only the dependencies, with the same profile and flags as the final build so their artifacts are reused.
RUN cargo chef cook --release --locked --workspace --recipe-path recipe.json

# Cargo loads every workspace member, so the build copies all of them.
COPY Cargo.toml Cargo.lock ./
COPY bot bot
COPY common common
COPY feed feed
COPY migrate migrate
COPY migrations migrations

# One build for every service compiles their shared dependencies once.
RUN cargo build --release --locked --workspace


# Distroless keeps glibc and CA certificates for the TLS clients, without a shell or package manager.
FROM gcr.io/distroless/cc-debian13 AS runtime

USER 10001:10001


FROM runtime AS bot

COPY --from=builder /build/target/release/bot /usr/local/bin/bot

EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/bot"]


FROM runtime AS feed

COPY --from=builder /build/target/release/feed /usr/local/bin/feed

ENTRYPOINT ["/usr/local/bin/feed"]


FROM runtime AS migration

COPY --from=builder /build/target/release/migrate /usr/local/bin/migrate

ENTRYPOINT ["/usr/local/bin/migrate"]
