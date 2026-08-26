from collections.abc import Callable

import httpx
import pytest

from src.infrastructure.telegram.media_downloader import TelegramMediaDownloader, UnsupportedPreviewMediaError

pytestmark = pytest.mark.unit


def create_downloader(
    monkeypatch: pytest.MonkeyPatch,
    handler: Callable[[httpx.Request], httpx.Response],
    max_bytes: int,
) -> TelegramMediaDownloader:
    """
    Create a media downloader backed by a deterministic HTTP transport.
    """
    async_client_class = httpx.AsyncClient
    transport = httpx.MockTransport(handler)

    def create_client(*, timeout: float) -> httpx.AsyncClient:
        """
        Create an HTTP client using the supplied mock transport.
        """
        return async_client_class(timeout=timeout, transport=transport)

    monkeypatch.setattr("src.infrastructure.telegram.media_downloader.httpx.AsyncClient", create_client)
    return TelegramMediaDownloader(10.0, max_bytes)


@pytest.mark.asyncio
async def test_download_media_applies_limit_to_whole_post(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify media files share one aggregate in-memory byte allowance.
    """

    def handle_request(_request: httpx.Request) -> httpx.Response:
        """
        Return one four-byte JPEG for every media request.
        """
        return httpx.Response(200, content=b"data", headers={"content-type": "image/jpeg"})

    downloader = create_downloader(monkeypatch, handle_request, max_bytes=7)
    with pytest.raises(UnsupportedPreviewMediaError):
        await downloader.download_media(["https://cdn.example/one.jpg", "https://cdn.example/two.jpg"])
    await downloader.close()


@pytest.mark.asyncio
async def test_download_media_accepts_files_within_aggregate_limit(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify all media files are retained when their combined size fits.
    """

    def handle_request(_request: httpx.Request) -> httpx.Response:
        """
        Return one four-byte JPEG for every media request.
        """
        return httpx.Response(200, content=b"data", headers={"content-type": "image/jpeg"})

    downloader = create_downloader(monkeypatch, handle_request, max_bytes=8)
    media = await downloader.download_media(["https://cdn.example/one.jpg", "https://cdn.example/two.jpg"])
    await downloader.close()

    assert [item.content for item in media] == [b"data", b"data"]
