from collections.abc import Callable

import httpx
import pytest

from src.infrastructure.telegram.errors import TelegramWebPreviewUnavailableError
from src.infrastructure.telegram.web_preview_client import TelegramWebPreviewClient

pytestmark = pytest.mark.unit


def install_mock_transport(
    monkeypatch: pytest.MonkeyPatch,
    handler: Callable[[httpx.Request], httpx.Response],
) -> None:
    """
    Install an HTTPX client factory backed by the supplied mock transport handler.
    """
    async_client_class = httpx.AsyncClient
    transport = httpx.MockTransport(handler)

    def create_client(*, timeout: float, follow_redirects: bool) -> httpx.AsyncClient:
        """
        Create an HTTPX client that routes requests through the mock transport.
        """
        return async_client_class(timeout=timeout, follow_redirects=follow_redirects, transport=transport)

    monkeypatch.setattr("src.infrastructure.telegram.web_preview_client.httpx.AsyncClient", create_client)


@pytest.mark.asyncio
async def test_fetch_channel_preview_uses_primary_host(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify a successful primary request does not contact the fallback host.
    """
    requested_urls: list[str] = []

    def handle_request(request: httpx.Request) -> httpx.Response:
        """
        Record the request and return a successful preview response.
        """
        requested_urls.append(str(request.url))
        return httpx.Response(200, text="primary preview")

    install_mock_transport(monkeypatch, handle_request)

    html = await TelegramWebPreviewClient(10.0).fetch_channel_preview("example")

    assert html == "primary preview"
    assert requested_urls == ["https://t.me/s/example"]


@pytest.mark.asyncio
async def test_fetch_channel_preview_falls_back_after_connect_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify a primary connection failure retries once through telegram.me.
    """
    requested_urls: list[str] = []

    def handle_request(request: httpx.Request) -> httpx.Response:
        """
        Fail the primary host and return the fallback preview response.
        """
        requested_urls.append(str(request.url))
        if request.url.host == "t.me":
            raise httpx.ConnectError("Name or service not known", request=request)
        return httpx.Response(200, text="fallback preview")

    install_mock_transport(monkeypatch, handle_request)

    html = await TelegramWebPreviewClient(10.0).fetch_channel_preview("example")

    assert html == "fallback preview"
    assert requested_urls == ["https://t.me/s/example", "https://telegram.me/s/example"]


@pytest.mark.asyncio
async def test_fetch_channel_preview_stops_after_both_hosts_fail(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify connection retries stop after the two supported preview hosts fail.
    """
    requested_urls: list[str] = []

    def handle_request(request: httpx.Request) -> httpx.Response:
        """
        Record each request and fail every connection attempt.
        """
        requested_urls.append(str(request.url))
        raise httpx.ConnectError("Name or service not known", request=request)

    install_mock_transport(monkeypatch, handle_request)

    with pytest.raises(TelegramWebPreviewUnavailableError):
        await TelegramWebPreviewClient(10.0).fetch_channel_preview("example")

    assert requested_urls == ["https://t.me/s/example", "https://telegram.me/s/example"]


@pytest.mark.asyncio
async def test_fetch_channel_preview_does_not_fail_over_after_http_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify HTTP response errors remain visible without switching preview hosts.
    """
    requested_urls: list[str] = []

    def handle_request(request: httpx.Request) -> httpx.Response:
        """
        Record the request and return an upstream server error.
        """
        requested_urls.append(str(request.url))
        return httpx.Response(503)

    install_mock_transport(monkeypatch, handle_request)

    with pytest.raises(TelegramWebPreviewUnavailableError):
        await TelegramWebPreviewClient(10.0).fetch_channel_preview("example")

    assert requested_urls == ["https://t.me/s/example"]


@pytest.mark.asyncio
@pytest.mark.parametrize("fail_primary", [False, True])
async def test_fetch_channel_preview_normalizes_read_timeout(
    monkeypatch: pytest.MonkeyPatch,
    fail_primary: bool,
) -> None:
    """Translate timeouts from either preview host into a per-channel availability failure."""

    def handle_request(request: httpx.Request) -> httpx.Response:
        """Fail the primary connection when requested and time out the selected host."""
        if fail_primary and request.url.host == "t.me":
            raise httpx.ConnectError("connection failed", request=request)
        raise httpx.ReadTimeout("read timed out", request=request)

    install_mock_transport(monkeypatch, handle_request)
    client = TelegramWebPreviewClient(10.0)
    try:
        with pytest.raises(TelegramWebPreviewUnavailableError) as error:
            await client.fetch_channel_preview("example")
        assert isinstance(error.value.__cause__, httpx.ReadTimeout)
    finally:
        await client.close()
