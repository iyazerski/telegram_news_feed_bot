FROM rust:1.99-slim-trixie AS builder

WORKDIR /build

# Cargo loads every workspace member, so the build copies all of them.
COPY Cargo.toml Cargo.lock ./
COPY bot bot
COPY common common
COPY feed feed
COPY migrate migrate
COPY recapper recapper

RUN cargo build --release --locked -p bot


# Distroless keeps glibc and CA certificates for the TLS clients, without a shell or package manager.
FROM gcr.io/distroless/cc-debian13 AS runtime

COPY --from=builder /build/target/release/bot /usr/local/bin/bot

USER 10001:10001

EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/bot"]
