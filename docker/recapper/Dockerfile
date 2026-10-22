FROM rust:1.99-slim-trixie AS builder

WORKDIR /build

COPY recapper/Cargo.toml recapper/Cargo.lock ./
COPY recapper/src src

RUN cargo build --release --locked


# Distroless keeps glibc and CA certificates for the TLS clients, without a shell or package manager.
FROM gcr.io/distroless/cc-debian13 AS runtime

COPY --from=builder /build/target/release/recapper /usr/local/bin/recapper

# The ChatGPT sign-in lives on a volume mounted here, so it survives restarts and new images.
ENV TELEGRAM_NEWS_RECAP_AUTH_DIR=/var/lib/telegram-news/auth
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/recapper"]
