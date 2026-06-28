import asyncio
from pathlib import Path
from typing import Annotated, Any

import httpx
import uvicorn
from fastapi import FastAPI, Header, HTTPException, Request
from fastapi.responses import RedirectResponse, Response
from fastapi.staticfiles import StaticFiles
from loguru import logger

from src.config.configs import AppConfigs
from src.entrypoints.bot.commands import BotCommandHandler
from src.entrypoints.bot.mini_app.api import create_mini_app_api_router
from src.infrastructure.database.orm import Database
from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.errors import TelegramApiError
from src.use_cases.manage_settings import SettingsService

MINI_APP_STATIC_DIR = Path(__file__).parent / "mini_app" / "static"


class BotRuntime:
    def __init__(
        self,
        configs: AppConfigs,
        telegram: TelegramBotApi,
        handler: BotCommandHandler,
        db: Database,
        settings: SettingsService,
    ) -> None:
        """
        Create the Telegram Bot API runtime and Mini App HTTP server.
        """
        self.configs = configs
        self.telegram = telegram
        self.handler = handler
        self.db = db
        self.settings = settings

    async def run_forever(self) -> None:
        """
        Run the configured Telegram update receiver forever.
        """
        await self.configure_menu_button()

        if self.configs.bot_update_mode == "webhook":
            await self.run_webhook_forever()
            return

        await asyncio.gather(self.run_http_server_forever(), self.run_polling_forever())

    async def configure_menu_button(self) -> None:
        """
        Configure the Telegram bot menu button when a public Mini App URL is available.
        """
        mini_app_url = self.resolve_mini_app_url()
        if not self.configs.bot_configure_menu_button or not mini_app_url:
            return

        await self.telegram.set_chat_menu_button("Manage Feed", mini_app_url)
        logger.info(f"Configured Telegram menu button for {mini_app_url}")

    async def run_polling_forever(self) -> None:
        """
        Poll Telegram bot updates forever for local development.
        """
        offset = 0
        logger.info("Bot command service started")
        if self.configs.bot_delete_webhook_on_polling_start:
            await self.telegram.delete_webhook()

        while True:
            try:
                updates = await self.telegram.get_updates(offset, self.configs.telegram_get_updates_timeout_seconds)
            except httpx.HTTPError as exc:
                logger.warning(f"Telegram getUpdates request failed: {exc}")
                await asyncio.sleep(self.configs.telegram_get_updates_idle_seconds)
                continue

            for update in updates:
                offset = update["update_id"] + 1
                await self.handle_update(update)
            await asyncio.sleep(self.configs.telegram_get_updates_idle_seconds)

    async def run_webhook_forever(self) -> None:
        """
        Receive Telegram bot updates through an HTTPS webhook behind the Kubernetes ingress.
        """
        webhook_url = self.resolve_webhook_url()
        await self.telegram.set_webhook(webhook_url, self.configs.bot_webhook_secret_token)
        logger.info(f"Bot webhook service started at {webhook_url}")
        await self.run_http_server_forever()

    async def run_http_server_forever(self) -> None:
        """
        Serve the FastAPI app for health checks, webhooks, API routes, and the Mini App.
        """
        app = self.create_app()
        server_config = uvicorn.Config(
            app,
            host=self.configs.bot_webhook_listen_host,
            port=self.configs.bot_webhook_port,
            log_level=self.configs.log_level.lower(),
        )
        server = uvicorn.Server(server_config)
        logger.info(
            f"Bot HTTP service started on {self.configs.bot_webhook_listen_host}:{self.configs.bot_webhook_port}"
        )
        await server.serve()

    def create_app(self) -> FastAPI:
        """
        Create the FastAPI app that serves health checks, webhooks, API routes, and the Mini App.
        """
        app = FastAPI(title="Telegram News Feed Bot")
        app.add_api_route("/healthz", self.handle_health_check, methods=["GET"])
        app.add_api_route(self.configs.bot_webhook_path, self.handle_webhook_request, methods=["POST"])
        app.include_router(create_mini_app_api_router(self.configs, self.db, self.settings))
        app.mount("/app", StaticFiles(directory=MINI_APP_STATIC_DIR, html=True), name="mini-app")

        @app.get("/", include_in_schema=False)
        def redirect_to_app() -> RedirectResponse:
            """
            Redirect browser visitors to the Mini App entrypoint.
            """
            return RedirectResponse("/app")

        return app

    def resolve_webhook_url(self) -> str:
        """
        Resolve the public HTTPS webhook URL registered with Telegram.
        """
        if self.configs.bot_webhook_url:
            return self.configs.bot_webhook_url

        if self.configs.public_host:
            return f"https://{self.configs.public_host.rstrip('/')}{self.configs.bot_webhook_path}"

        raise ValueError("TELEGRAM_NEWS_PUBLIC_HOST or TELEGRAM_NEWS_BOT_WEBHOOK_URL is required in webhook mode")

    def resolve_mini_app_url(self) -> str:
        """
        Resolve the public Mini App URL used for the Telegram menu button.
        """
        if self.configs.mini_app_url:
            return self.configs.mini_app_url

        if self.configs.public_host:
            return f"https://{self.configs.public_host.rstrip('/')}/app"

        return ""

    async def handle_health_check(self) -> Response:
        """
        Return a Kubernetes health-check response.
        """
        return Response(content="ok", media_type="text/plain")

    async def handle_webhook_request(
        self,
        request: Request,
        x_telegram_bot_api_secret_token: Annotated[
            str | None,
            Header(alias="X-Telegram-Bot-Api-Secret-Token"),
        ] = None,
    ) -> Response:
        """
        Validate and process one Telegram webhook HTTP request.
        """
        if (
            self.configs.bot_webhook_secret_token
            and x_telegram_bot_api_secret_token != self.configs.bot_webhook_secret_token
        ):
            raise HTTPException(status_code=401, detail="unauthorized")

        update: dict[str, Any] = await request.json()
        await self.handle_update(update)
        return Response(content="ok", media_type="text/plain")

    async def handle_update(self, update: dict[str, Any]) -> None:
        """
        Handle one Telegram update when it contains a text command message.
        """
        message = update.get("message")
        if message is None:
            return

        text = message.get("text")
        if text is None:
            return

        from_user = message["from"]
        chat = message["chat"]
        chat_id = chat["id"]
        if self.configs.admin_user_id and str(from_user["id"]) != self.configs.admin_user_id:
            await self.send_response(chat_id, "This bot is restricted to its configured admin.")
            return

        command = text.strip().split(maxsplit=1)[0].split("@", maxsplit=1)[0].lower()
        if command == "/start":
            response = self.handler.start(chat_id)

        await self.send_response(chat_id, response)

    async def send_response(self, chat_id: int | str, response: str) -> None:
        """
        Send a plain bot command response without stopping the command runtime on Telegram errors.
        """
        try:
            await self.telegram.send_text_message(chat_id, response)
        except TelegramApiError as exc:
            logger.error(f"Telegram rejected command response for chat {chat_id}: {exc.description}")
