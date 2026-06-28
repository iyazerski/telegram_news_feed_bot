import httpx

from src.infrastructure.telegram.uploads import TelegramUpload

MEDIA_GROUP_LIMIT = 10
SUPPORTED_MEDIA_CONTENT_TYPES = {"image/jpeg": "jpg", "image/png": "png"}


class UnsupportedPreviewMediaError(Exception):
    """Represent preview media that cannot be uploaded as a Telegram photo."""


class TelegramMediaDownloader:
    def __init__(self, timeout_seconds: float, max_bytes: int) -> None:
        """
        Create a downloader for Telegram preview media that can become Bot API uploads.
        """
        self.timeout_seconds = timeout_seconds
        self.max_bytes = max_bytes

    async def download_media(self, media_urls: list[str]) -> list[TelegramUpload]:
        """
        Download preview media and prepare multipart upload files for Telegram delivery.
        """
        media: list[TelegramUpload] = []
        async with httpx.AsyncClient(timeout=self.timeout_seconds) as client:
            for index, media_url in enumerate(media_urls[:MEDIA_GROUP_LIMIT]):
                content, content_type = await self.download_media_file(client, media_url)
                media.append(
                    TelegramUpload(
                        field_name=f"media{index}",
                        filename=f"telegram-media-{index}.{SUPPORTED_MEDIA_CONTENT_TYPES[content_type]}",
                        content=content,
                        content_type=content_type,
                    )
                )

        return media

    async def download_media_file(self, client: httpx.AsyncClient, media_url: str) -> tuple[bytes, str]:
        """
        Download one preview media file with a strict in-memory size cap.
        """
        content = bytearray()
        async with client.stream("GET", media_url) as response:
            response.raise_for_status()
            raw_content_type = response.headers.get("content-type")
            if raw_content_type is None:
                raise UnsupportedPreviewMediaError("Telegram preview media response has no content type")

            content_type = self.extract_supported_content_type(raw_content_type)
            raw_content_length = response.headers.get("content-length")
            if raw_content_length is not None:
                if not raw_content_length.isdecimal():
                    raise UnsupportedPreviewMediaError("Telegram preview media response has invalid content length")
                if int(raw_content_length) > self.max_bytes:
                    raise UnsupportedPreviewMediaError("Telegram preview media response is too large")

            async for chunk in response.aiter_bytes():
                content.extend(chunk)
                if len(content) > self.max_bytes:
                    raise UnsupportedPreviewMediaError("Telegram preview media response is too large")

        return bytes(content), content_type

    def extract_supported_content_type(self, raw_content_type: str) -> str:
        """
        Validate and normalize a downloaded media content type for Telegram photo upload.
        """
        content_type = raw_content_type.split(";", maxsplit=1)[0].lower()
        if content_type not in SUPPORTED_MEDIA_CONTENT_TYPES:
            raise UnsupportedPreviewMediaError(f"Unsupported Telegram preview media type: {content_type}")
        return content_type
