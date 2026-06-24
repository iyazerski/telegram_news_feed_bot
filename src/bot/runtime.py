import asyncio
from typing import Any

import httpx
from aiohttp import web
from loguru import logger

from src.bot.commands import BotCommandHandler
from src.configs.configs import AppConfigs
from src.services.telegram_api import TelegramApiError, TelegramBotApi


class BotRuntime:
    def __init__(self, configs: AppConfigs, telegram: TelegramBotApi, handler: BotCommandHandler) -> None:
        """
        Create the Telegram Bot API polling runtime.
        """
        self.configs = configs
        self.telegram = telegram
        self.handler = handler

    async def run_forever(self) -> None:
        """
        Run the configured Telegram update receiver forever.
        """
        if self.configs.bot_update_mode == "webhook":
            await self.run_webhook_forever()
            return

        await self.run_polling_forever()

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
        app = web.Application()
        app.router.add_get("/healthz", self.handle_health_check)
        app.router.add_post(self.configs.bot_webhook_path, self.handle_webhook_request)

        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, self.configs.bot_webhook_listen_host, self.configs.bot_webhook_port)
        await site.start()

        await self.telegram.set_webhook(webhook_url, self.configs.bot_webhook_secret_token)
        logger.info(f"Bot webhook service started at {webhook_url}")

        try:
            await asyncio.Event().wait()
        finally:
            await runner.cleanup()

    def resolve_webhook_url(self) -> str:
        """
        Resolve the public HTTPS webhook URL registered with Telegram.
        """
        if self.configs.bot_webhook_url:
            return self.configs.bot_webhook_url

        if self.configs.public_host:
            return f"https://{self.configs.public_host.rstrip('/')}{self.configs.bot_webhook_path}"

        raise ValueError("TELEGRAM_NEWS_PUBLIC_HOST or TELEGRAM_NEWS_BOT_WEBHOOK_URL is required in webhook mode")

    async def handle_health_check(self, _request: web.Request) -> web.Response:
        """
        Return a Kubernetes health-check response.
        """
        return web.Response(text="ok")

    async def handle_webhook_request(self, request: web.Request) -> web.Response:
        """
        Validate and process one Telegram webhook HTTP request.
        """
        if self.configs.bot_webhook_secret_token:
            secret_token = request.headers.get("X-Telegram-Bot-Api-Secret-Token")
            if secret_token != self.configs.bot_webhook_secret_token:
                return web.Response(status=401, text="unauthorized")

        update = await request.json()
        await self.handle_update(update)
        return web.Response(text="ok")

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
        else:
            response = self.handler.handle_text(text)
        await self.send_response(chat_id, response)

    async def send_response(self, chat_id: int | str, response: str) -> None:
        """
        Send a plain bot command response without stopping the command runtime on Telegram errors.
        """
        try:
            await self.telegram.send_text_message(chat_id, response)
        except TelegramApiError as exc:
            logger.error(f"Telegram rejected command response for chat {chat_id}: {exc.description}")
