import json
from typing import Any

import httpx

from src.models.media import TelegramUpload

TelegramFiles = dict[str, tuple[str, bytes, str]]


class TelegramApiError(Exception):
    def __init__(self, method: str, description: str, error_code: int) -> None:
        """
        Create an exception describing a Telegram Bot API failure.
        """
        super().__init__(f"{method} failed: {description}")
        self.method = method
        self.description = description
        self.error_code = error_code


class TelegramBotApi:
    def __init__(self, token: str, timeout_seconds: float) -> None:
        """
        Create a Telegram Bot API client for one bot token.
        """
        self.base_url = f"https://api.telegram.org/bot{token}"
        self.timeout_seconds = timeout_seconds

    async def get_updates(self, offset: int, timeout_seconds: int) -> list[dict[str, Any]]:
        """
        Fetch Telegram bot updates using long polling.
        """
        payload = {"offset": offset, "timeout": timeout_seconds, "allowed_updates": ["message"]}
        response = await self._post("getUpdates", payload, timeout_seconds + self.timeout_seconds)
        return response["result"]

    async def set_webhook(self, url: str, secret_token: str) -> None:
        """
        Register the production HTTPS webhook URL for Telegram bot updates.
        """
        payload: dict[str, Any] = {"url": url, "allowed_updates": ["message"]}
        if secret_token:
            payload["secret_token"] = secret_token

        await self._post("setWebhook", payload)

    async def delete_webhook(self) -> None:
        """
        Remove any registered webhook before local long polling starts.
        """
        await self._post("deleteWebhook", {"drop_pending_updates": False})

    async def send_text_message(self, chat_id: int | str, text: str) -> None:
        """
        Send a plain text message to a Telegram chat.
        """
        await self._post(
            "sendMessage",
            {"chat_id": chat_id, "text": text, "disable_web_page_preview": False},
        )

    async def send_html_message(self, chat_id: int | str, text: str) -> None:
        """
        Send an HTML text message to a Telegram chat.
        """
        await self._post(
            "sendMessage",
            {"chat_id": chat_id, "text": text, "parse_mode": "HTML", "disable_web_page_preview": False},
        )

    async def send_photo(self, chat_id: int | str, photo: TelegramUpload, caption: str) -> None:
        """
        Send an uploaded photo message with an HTML caption to a Telegram chat.
        """
        await self._post(
            "sendPhoto",
            {"chat_id": chat_id, "caption": caption, "parse_mode": "HTML"},
            files={"photo": (photo.filename, photo.content, photo.content_type)},
        )

    async def send_media_group(self, chat_id: int | str, photos: list[TelegramUpload], caption: str) -> None:
        """
        Send uploaded photos as one grouped album message to a Telegram chat.
        """
        media: list[dict[str, str]] = []
        files: TelegramFiles = {}
        for index, photo in enumerate(photos):
            media_item = {"type": "photo", "media": f"attach://{photo.field_name}"}
            if index == 0:
                media_item["caption"] = caption
                media_item["parse_mode"] = "HTML"

            media.append(media_item)
            files[photo.field_name] = (photo.filename, photo.content, photo.content_type)

        await self._post("sendMediaGroup", {"chat_id": chat_id, "media": json.dumps(media)}, files=files)

    async def _post(
        self,
        method: str,
        payload: dict[str, Any],
        timeout_seconds: float | None = None,
        files: TelegramFiles | None = None,
    ) -> dict[str, Any]:
        """
        Call a Telegram Bot API method and return the decoded JSON response.
        """
        request_timeout_seconds = self.timeout_seconds
        if timeout_seconds is not None:
            request_timeout_seconds = timeout_seconds

        async with httpx.AsyncClient(timeout=request_timeout_seconds) as client:
            if files is None:
                response = await client.post(f"{self.base_url}/{method}", json=payload)
            else:
                response = await client.post(f"{self.base_url}/{method}", data=payload, files=files)
            data = response.json()

        if not data["ok"]:
            raise TelegramApiError(method, data["description"], data["error_code"])

        response.raise_for_status()

        return data
