import json
from typing import Any

import httpx

from src.infrastructure.telegram.errors import TelegramApiError
from src.infrastructure.telegram.uploads import TelegramUpload

TelegramFiles = dict[str, tuple[str, bytes, str]]


class TelegramBotApi:
    def __init__(self, token: str, timeout_seconds: float) -> None:
        """
        Create a Telegram Bot API client for one bot token.
        """
        self.base_url = f"https://api.telegram.org/bot{token}"
        self.timeout_seconds = timeout_seconds
        self.client = httpx.AsyncClient(timeout=timeout_seconds)

    async def close(self) -> None:
        """
        Close the persistent Telegram HTTP client.
        """
        await self.client.aclose()

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

    async def set_chat_menu_button(self, text: str, web_app_url: str) -> None:
        """
        Configure the default bot menu button to launch the Mini App.
        """
        await self._post(
            "setChatMenuButton",
            {
                "menu_button": {
                    "type": "web_app",
                    "text": text,
                    "web_app": {"url": web_app_url},
                }
            },
        )

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

    async def send_video(self, chat_id: int | str, video: TelegramUpload, caption: str) -> None:
        """
        Send an uploaded video message with an HTML caption to a Telegram chat.
        """
        await self._post(
            "sendVideo",
            {"chat_id": chat_id, "caption": caption, "parse_mode": "HTML"},
            files={"video": (video.filename, video.content, video.content_type)},
        )

    async def send_media_group(self, chat_id: int | str, media: list[TelegramUpload], caption: str) -> None:
        """
        Send uploaded photos and videos as one grouped album message to a Telegram chat.
        """
        media_payload: list[dict[str, str]] = []
        files: TelegramFiles = {}
        for index, media_file in enumerate(media):
            media_type = "video" if media_file.content_type.startswith("video/") else "photo"
            media_item = {"type": media_type, "media": f"attach://{media_file.field_name}"}
            if index == 0:
                media_item["caption"] = caption
                media_item["parse_mode"] = "HTML"

            media_payload.append(media_item)
            files[media_file.field_name] = (media_file.filename, media_file.content, media_file.content_type)

        await self._post("sendMediaGroup", {"chat_id": chat_id, "media": json.dumps(media_payload)}, files=files)

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

        if files is None:
            response = await self.client.post(
                f"{self.base_url}/{method}",
                json=payload,
                timeout=request_timeout_seconds,
            )
        else:
            response = await self.client.post(
                f"{self.base_url}/{method}",
                data=payload,
                files=files,
                timeout=request_timeout_seconds,
            )

        data = self._decode_response(method, response)
        if not data["ok"]:
            raise TelegramApiError(method, data["description"], data["error_code"])

        response.raise_for_status()

        return data

    def _decode_response(self, method: str, response: httpx.Response) -> dict[str, Any]:
        """
        Decode and validate one Telegram Bot API response envelope.
        """
        try:
            data = response.json()
        except ValueError as exc:
            raise httpx.RemoteProtocolError(
                f"{method} returned an invalid JSON response",
                request=response.request,
            ) from exc

        if not isinstance(data, dict) or not isinstance(data.get("ok"), bool):
            raise httpx.RemoteProtocolError(
                f"{method} returned an invalid response envelope",
                request=response.request,
            )

        if not data["ok"] and (
            not isinstance(data.get("description"), str) or not isinstance(data.get("error_code"), int)
        ):
            raise httpx.RemoteProtocolError(
                f"{method} returned an invalid error response",
                request=response.request,
            )

        return data
