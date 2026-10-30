# Telegram News Feed Bot

Self-hosted Telegram public channel feed aggregator. It discovers public `t.me/s/<channel>` posts and reposts each source post into your direct chat with the bot. Text formatting, links, and preview-exposed images are preserved where Telegram's Bot API limits allow; multi-image posts are sent as one grouped album.

## Services

- `bot`: A Rust service in `bot/` with the Telegram command UI, webhook receiver, and Mini App API/frontend for managing source channels and the polling interval.
- `feed`: A Rust service in `feed/` that periodically checks the public web preview pages of your channels and reposts each new post as one Telegram bot delivery, in order per channel.
- `postgres`: Persistent source subscriptions, bot-chat destination, and per-channel committed cursors.

## Mini App

The bot service serves a Telegram Mini App at `/app/` and authenticated JSON API routes at `/api/*`. The Mini App files are built into the bot binary. The API validates Telegram `initData` and serves only `TELEGRAM_NEWS_ADMIN_USER_ID`, which the bot requires.

Set `TELEGRAM_NEWS_MINI_APP_URL` to a public HTTPS URL when running a local test bot through a tunnel. In production, the menu button defaults to `https://<TELEGRAM_NEWS_PUBLIC_HOST>/app`. Set `TELEGRAM_NEWS_BOT_CONFIGURE_MENU_BUTTON=false` to skip automatic menu-button configuration.

## Commands

```text
/start
```

`/start` connects your chat to the feed. Blocking the bot pauses it: the feed stops checking channels. Your channels and settings stay saved, and pressing Start again resumes where the feed left off.

## Local Development

```bash
cargo run -p migrate
cargo run -p bot
cargo run -p feed
```

Checks run with [prek](https://prek.j178.dev): `prek install` adds the Git hook, and `prek run --all-files` runs every check, as CI does.

All services read the same `.env` file.

The bot defaults to long polling locally with `TELEGRAM_NEWS_BOT_UPDATE_MODE=polling` and also serves the Mini App/API on `http://127.0.0.1:8000`. Production Kubernetes config sets `TELEGRAM_NEWS_BOT_UPDATE_MODE=webhook` and registers `https://newsfeedbot.iyazerski.dev/telegram/webhook` with Telegram. Telegram allows only one update mode per bot token; set `TELEGRAM_NEWS_BOT_DELETE_WEBHOOK_ON_POLLING_START=true` locally only when you intentionally want local polling to take over that token.

## Code Layout

The services form one Cargo workspace. `bot/src` and `feed/src` use the same layers: `domain`, `infrastructure`, `use_cases`, and `entrypoints`, with unit tests next to the code. `common/` holds what they share: settings loading, logging, shutdown, the database pool, the stored app settings, and the Telegram Bot API client. `migrations/` holds the SQL migrations of the database schema that all services share. `migrate/` builds them into the binary that the deploy runs as a Kubernetes job before it updates the services.

## Database Migrations

Create a migration with [sqlx-cli](https://crates.io/crates/sqlx-cli), then write its SQL:

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
sqlx migrate add <description>
```

Apply migrations locally with `cargo run -p migrate`. Migrations are forward-only and each runs in a transaction.
