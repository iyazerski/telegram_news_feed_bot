# check=skip=SecretsUsedInArgOrEnv
# TELEGRAM_NEWS_RECAP_AUTH_DIR names a directory, not a secret, but its name trips that check.

FROM rust:1.99-slim-trixie AS builder

WORKDIR /build

# Cargo loads every workspace member, so the build copies all of them.
COPY Cargo.toml Cargo.lock ./
COPY bot bot
COPY common common
COPY feed feed
COPY migrate migrate
COPY migrations migrations
COPY recapper recapper

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


FROM runtime AS recapper

COPY --from=builder /build/target/release/recapper /usr/local/bin/recapper

# The ChatGPT sign-in lives on a volume mounted here, so it survives restarts and new images.
ENV TELEGRAM_NEWS_RECAP_AUTH_DIR=/var/lib/telegram-news/auth

ENTRYPOINT ["/usr/local/bin/recapper"]
