# Telegram News Feed Bot

Self-hosted Telegram public channel feed aggregator. It discovers public `t.me/s/<channel>` posts, publishes structured post metadata through NATS, and reposts each source post into your direct chat with the bot. Text formatting, links, and preview-exposed images are preserved where Telegram's Bot API limits allow; multi-image posts are sent as one grouped album.

## Services

- `bot`: Telegram command UI for managing source channels and polling interval.
- `poller`: Periodically checks public Telegram web preview pages and publishes structured post metadata.
- `dispatcher`: Consumes post metadata and reposts each source post as one Telegram bot delivery.
- `postgres`: Persistent source subscriptions, bot-chat destination, and per-channel committed cursors.
- `nats`: Lightweight message bus between poller and dispatcher.

## Commands

```text
/start
/add_channel <channel_username_or_url>
/remove_channel <channel_username>
/list_channels
/set_interval <seconds_or_minutes>
/status
```

## Local Development

```bash
uv sync --group dev
uv run alembic upgrade head
uv run python -m src.bot.main
uv run python -m src.poller.main
uv run python -m src.dispatcher.main
```

The bot defaults to long polling locally with `TELEGRAM_NEWS_BOT_UPDATE_MODE=polling`. Production Kubernetes config sets `TELEGRAM_NEWS_BOT_UPDATE_MODE=webhook` and registers `https://newsfeedbot.iyazerski.dev/telegram/webhook` with Telegram. Telegram allows only one update mode per bot token; set `TELEGRAM_NEWS_BOT_DELETE_WEBHOOK_ON_POLLING_START=true` locally only when you intentionally want local polling to take over that token.

## Code Layout

- `src/bot`, `src/poller`, and `src/dispatcher` contain microservice runtimes and public handlers.
- `src/services` contains shared business and integration services.
- `src/models` contains internal data models, Pydantic schemas, dataclasses, and SQLAlchemy table models.
- `tests/unit` mirrors the `src` package layout for focused unit coverage.
