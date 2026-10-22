# Telegram News Feed Bot

Self-hosted Telegram public channel feed aggregator. It discovers public `t.me/s/<channel>` posts and reposts each source post into your direct chat with the bot. Text formatting, links, and preview-exposed images are preserved where Telegram's Bot API limits allow; multi-image posts are sent as one grouped album.

It can also send AI news recaps: every few hours, an AI agent researches the web for the most important news on your topics and sends 3 to 5 short stories with source links in your language.

## Services

- `bot`: A Rust service in `bot/` with the Telegram command UI, webhook receiver, and Mini App API/frontend for managing source channels, the polling interval, and AI recaps.
- `feed`: A Rust service in `feed/` that periodically checks the public web preview pages of your channels and reposts each new post as one Telegram bot delivery, in order per channel.
- `recapper`: A Rust service in `recapper/` that sends AI news recaps on schedule. It runs a [Rig](https://rig.rs) agent on `gpt-6-luna` with medium reasoning, signed in with your ChatGPT account, and searches the web through [Tavily](https://tavily.com).
- `postgres`: Persistent source subscriptions, bot-chat destination, and per-channel committed cursors.

## Mini App

The bot service serves a Telegram Mini App at `/app/` and authenticated JSON API routes at `/api/*`. The Mini App files are built into the bot binary. The API validates Telegram `initData` and serves only `TELEGRAM_NEWS_ADMIN_USER_ID`, which the bot requires.

Set `TELEGRAM_NEWS_MINI_APP_URL` to a public HTTPS URL when running a local test bot through a tunnel. In production, the menu button defaults to `https://<TELEGRAM_NEWS_PUBLIC_HOST>/app`. Set `TELEGRAM_NEWS_BOT_CONFIGURE_MENU_BUTTON=false` to skip automatic menu-button configuration.

## AI Recaps

Set topics, language, and frequency in the Mini App's AI Recap section. Turning recaps on sends the first one within a minute, and Send now requests one right away. Each recap researches at least the last 24 hours and skips stories that earlier recaps sent.

The recapper keeps its ChatGPT sign-in, which it refreshes itself, on the `telegram-news-recapper-auth-pvc` volume. To sign in, or to sign in again after the bot reports a lost sign-in, run this and enter the shown code at the shown URL:

```bash
kubectl exec -it deploy/telegram-news-recapper -- recapper login
```

Web search needs a Tavily API key. CD reads it from the `TAVILY_API_KEY` GitHub Actions secret and passes it to the recapper as `TELEGRAM_NEWS_RECAP_TAVILY_API_KEY`.

## Commands

```text
/start
```

## Local Development

```bash
cargo run -p migrate
cargo run -p bot
cargo run -p feed
cargo run -p recapper -- login   # once, to sign in to ChatGPT
cargo run -p recapper
```

All services read the same `.env` file. The local recapper also needs `TELEGRAM_NEWS_RECAP_TAVILY_API_KEY`, and keeps its ChatGPT sign-in in `.recapper-auth/`.

The bot defaults to long polling locally with `TELEGRAM_NEWS_BOT_UPDATE_MODE=polling` and also serves the Mini App/API on `http://127.0.0.1:8000`. Production Kubernetes config sets `TELEGRAM_NEWS_BOT_UPDATE_MODE=webhook` and registers `https://newsfeedbot.iyazerski.dev/telegram/webhook` with Telegram. Telegram allows only one update mode per bot token; set `TELEGRAM_NEWS_BOT_DELETE_WEBHOOK_ON_POLLING_START=true` locally only when you intentionally want local polling to take over that token.

## Code Layout

The services form one Cargo workspace. `bot/src`, `feed/src`, and `recapper/src` use the same layers: `domain`, `infrastructure`, `use_cases`, and `entrypoints`, with unit tests next to the code. `common/` holds what they share: settings loading, logging, shutdown, the database pool, the stored app settings, and the Telegram Bot API client. `migrations/` holds the SQL migrations of the database schema that all services share. `migrate/` builds them into the binary that the deploy runs as a Kubernetes job before it updates the services.

## Database Migrations

Create a migration with [sqlx-cli](https://crates.io/crates/sqlx-cli), then write its SQL:

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
sqlx migrate add <description>
```

Apply migrations locally with `cargo run -p migrate`. Migrations are forward-only and each runs in a transaction.
