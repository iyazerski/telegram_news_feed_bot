# Telegram News Feed Bot

Self-hosted Telegram public channel feed aggregator. It discovers public `t.me/s/<channel>` posts, publishes structured post metadata through NATS, and reposts each source post into your direct chat with the bot. Text formatting, links, and preview-exposed images are preserved where Telegram's Bot API limits allow; multi-image posts are sent as one grouped album.

It can also send AI news recaps: every few hours, Pi researches the web for the most important news on your topics and sends 3 to 5 short stories with source links in your language.

## Services

- `bot`: Telegram command UI, webhook receiver, and Mini App API/frontend for managing source channels and polling interval.
- `poller`: Periodically checks public Telegram web preview pages and publishes structured post metadata.
- `dispatcher`: Consumes post metadata and reposts each source post as one Telegram bot delivery.
- `recapper`: Runs [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt) with [pi-web-access](https://www.npmjs.com/package/pi-web-access) on schedule and sends AI news recaps. Pi uses your ChatGPT account through Sign in with ChatGPT.
- `postgres`: Persistent source subscriptions, bot-chat destination, and per-channel committed cursors.
- `nats`: Lightweight message bus between poller and dispatcher.

## Mini App

The bot service serves a Telegram Mini App at `/app` and authenticated JSON API routes at `/api/*`. The Mini App validates Telegram `initData` on the backend and restricts access to `TELEGRAM_NEWS_ADMIN_USER_ID` when configured.

Set `TELEGRAM_NEWS_MINI_APP_URL` to a public HTTPS URL when running a local test bot through a tunnel. In production, the menu button defaults to `https://<TELEGRAM_NEWS_PUBLIC_HOST>/app`. Set `TELEGRAM_NEWS_BOT_CONFIGURE_MENU_BUTTON=false` to skip automatic menu-button configuration.

## AI Recaps

Set topics, language, frequency, model, and reasoning level in the Mini App's AI Recap section. Turning recaps on sends the first one within a minute, and Send now requests one right away. Each recap researches at least the last 24 hours and skips stories that earlier recaps sent. The model list comes from the recapper, which asks Pi for the signed-in account's models every hour.

The recapper keeps Pi's home, including the ChatGPT credential that Pi refreshes, on the `telegram-news-pi-home-pvc` volume. To sign in, or to sign in again after the bot reports a lost sign-in:

```bash
export PI_CODING_AGENT_DIR="$(mktemp -d)"
pi-bolt   # run /login, choose OpenAI (Sign in with ChatGPT), then quit
pod="$(kubectl get pod -l component=recapper -o name | head -n 1)"
kubectl cp "$PI_CODING_AGENT_DIR/auth.json" "${pod#pod/}:/var/lib/telegram-news/pi/auth.json"
rm -rf "$PI_CODING_AGENT_DIR"
```

Delete the local copy after copying it, so two Pi installations don't race to refresh the same token. `TELEGRAM_NEWS_RECAP_PROVIDER` selects the Pi provider and defaults to `openai`.

## Commands

```text
/start
```

## Local Development

```bash
uv sync --group dev
uv run alembic upgrade head
uv run python -m src.entrypoints.bot.main
uv run python -m src.entrypoints.poller.main
uv run python -m src.entrypoints.dispatcher.main
uv run python -m src.entrypoints.recapper.main
```

The local recapper uses `pi` from `PATH` with your default Pi home. Install web search once with `PI_CODING_AGENT_DIR=/tmp/pi-packages pi-bolt install npm:pi-web-access@0.43.0` and set `TELEGRAM_NEWS_RECAP_PI_WEB_ACCESS_PATH=/tmp/pi-packages/npm/node_modules/pi-web-access`.

The bot defaults to long polling locally with `TELEGRAM_NEWS_BOT_UPDATE_MODE=polling` and also serves the Mini App/API on `http://127.0.0.1:8000`. Production Kubernetes config sets `TELEGRAM_NEWS_BOT_UPDATE_MODE=webhook` and registers `https://newsfeedbot.iyazerski.dev/telegram/webhook` with Telegram. Telegram allows only one update mode per bot token; set `TELEGRAM_NEWS_BOT_DELETE_WEBHOOK_ON_POLLING_START=true` locally only when you intentionally want local polling to take over that token.

## Code Layout

- `src/entrypoints` contains microservice runtimes, HTTP routes, commands, and process wiring.
- `src/use_cases` contains workflows such as channel management, polling discovery, and post delivery.
- `src/domain` contains transport-free business concepts and decisions.
- `src/infrastructure` contains database, NATS, Telegram Bot API, web-preview parsing, and other adapters.
- `tests/unit` mirrors the `src` package layout for focused unit coverage.
