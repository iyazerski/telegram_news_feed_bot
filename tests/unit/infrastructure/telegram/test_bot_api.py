from collections.abc import Callable

import httpx
import pytest

from src.infrastructure.telegram.bot_api import TelegramBotApi
from src.infrastructure.telegram.errors import TelegramApiError

pytestmark = pytest.mark.unit


def create_api(
    monkeypatch: pytest.MonkeyPatch,
    handler: Callable[[httpx.Request], httpx.Response],
) -> TelegramBotApi:
    """
    Create a Telegram Bot API client backed by a deterministic HTTP transport.
    """
    async_client_class = httpx.AsyncClient
    transport = httpx.MockTransport(handler)

    def create_client(*, timeout: float) -> httpx.AsyncClient:
        """
        Create an HTTP client using the supplied mock transport.
        """
        return async_client_class(timeout=timeout, transport=transport)

    monkeypatch.setattr("src.infrastructure.telegram.bot_api.httpx.AsyncClient", create_client)
    return TelegramBotApi("token", 10.0)


@pytest.mark.asyncio
async def test_client_reuses_transport_for_multiple_requests(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify multiple Telegram calls use the persistent client.
    """
    requested_methods: list[str] = []

    def handle_request(request: httpx.Request) -> httpx.Response:
        """
        Record each Telegram method and return success.
        """
        requested_methods.append(request.url.path.rsplit("/", maxsplit=1)[-1])
        return httpx.Response(200, json={"ok": True, "result": {}})

    api = create_api(monkeypatch, handle_request)
    await api.send_text_message(1, "one")
    await api.send_text_message(1, "two")
    await api.close()

    assert requested_methods == ["sendMessage", "sendMessage"]


@pytest.mark.asyncio
async def test_non_json_error_is_retryable_http_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify proxy error pages become transport errors instead of decoding failures.
    """

    def handle_request(_request: httpx.Request) -> httpx.Response:
        """
        Return a non-JSON upstream failure.
        """
        return httpx.Response(502, text="bad gateway")

    api = create_api(monkeypatch, handle_request)
    with pytest.raises(httpx.HTTPError):
        await api.send_text_message(1, "hello")
    await api.close()


@pytest.mark.asyncio
async def test_valid_telegram_error_keeps_api_details(monkeypatch: pytest.MonkeyPatch) -> None:
    """
    Verify valid Telegram failures remain classified by their API error code.
    """

    def handle_request(_request: httpx.Request) -> httpx.Response:
        """
        Return a structured Telegram rate-limit response.
        """
        return httpx.Response(429, json={"ok": False, "description": "retry later", "error_code": 429})

    api = create_api(monkeypatch, handle_request)
    with pytest.raises(TelegramApiError) as error:
        await api.send_text_message(1, "hello")
    await api.close()

    assert error.value.error_code == 429
