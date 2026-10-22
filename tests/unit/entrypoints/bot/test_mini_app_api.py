import hashlib
import hmac
import json
import time
from collections.abc import AsyncIterator
from urllib.parse import urlencode

import httpx
import pytest
import pytest_asyncio
from fastapi import FastAPI

from src.config.configs import AppConfigs
from src.entrypoints.bot.mini_app.api import CHANNEL_PAGE_SIZE, create_mini_app_api_router
from src.infrastructure.database.models import SourceChannel
from src.infrastructure.database.orm import Database
from src.use_cases.manage_settings import SettingsService

pytestmark = pytest.mark.unit


@pytest_asyncio.fixture
async def api_client(database: Database) -> AsyncIterator[httpx.AsyncClient]:
    """Create an authenticated Mini App client with isolated configuration and storage."""
    token = "test-token"
    configs = AppConfigs(bot_token=token, admin_user_id="123")
    fields = {"auth_date": str(int(time.time())), "user": json.dumps({"id": 123, "first_name": "Test"})}
    secret = hmac.new(b"WebAppData", configs.bot_token.encode(), hashlib.sha256).digest()
    fields["hash"] = hmac.new(
        secret,
        "\n".join(f"{key}={fields[key]}" for key in sorted(fields)).encode(),
        hashlib.sha256,
    ).hexdigest()
    app = FastAPI()
    app.include_router(create_mini_app_api_router(configs, database, SettingsService()))
    async with httpx.AsyncClient(
        transport=httpx.ASGITransport(app=app),
        base_url="http://test",
        headers={"X-Telegram-Init-Data": urlencode(fields)},
    ) as client:
        yield client


@pytest.mark.asyncio
async def test_channel_pages_are_bounded_and_exclude_inactive_rows(
    api_client: httpx.AsyncClient, database: Database
) -> None:
    """Walk active subscriptions across page boundaries without duplicates or inactive rows."""
    usernames = [f"channel{index:04d}" for index in range(CHANNEL_PAGE_SIZE + 2)]
    with database.create_session() as session:
        session.add_all(SourceChannel(username=username) for username in reversed(usernames))
        session.add(SourceChannel(username="channel0000inactive", active=False))
        session.commit()
    response = await api_client.get("/api/state")
    assert response.status_code == 200
    first = response.json()
    assert [channel["username"] for channel in first["channels"]] == usernames[:CHANNEL_PAGE_SIZE]
    assert first["next_cursor"] == usernames[CHANNEL_PAGE_SIZE - 1]
    second = (await api_client.get("/api/channels", params={"after": first["next_cursor"]})).json()
    assert [channel["username"] for channel in second["channels"]] == usernames[CHANNEL_PAGE_SIZE:]
    assert second["next_cursor"] is None


@pytest.mark.asyncio
async def test_mutations_return_only_changed_resources(api_client: httpx.AsyncClient) -> None:
    """Update subscriptions and settings without returning the complete dashboard."""
    channel = {"username": "example", "url": "https://t.me/example"}
    response = await api_client.post("/api/channels", json={"username_or_url": "@example"})
    assert response.status_code == 200
    assert response.json() == channel
    assert (await api_client.post("/api/channels", json={"username_or_url": "example"})).json() == channel
    response = await api_client.patch("/api/settings/poll-interval", json={"interval": "15m"})
    assert response.status_code == 200
    assert response.json() == {"poll_interval_seconds": 900}
    response = await api_client.delete("/api/channels/example")
    assert response.status_code == 204
    assert response.content == b""
    state = (await api_client.get("/api/state")).json()
    assert (state["channels"], state["next_cursor"], state["poll_interval_seconds"]) == ([], None, 900)
    assert (await api_client.post("/api/channels", json={"username_or_url": "example"})).json() == channel


@pytest.mark.asyncio
async def test_recap_settings_schedule_recaps_and_validate_input(api_client: httpx.AsyncClient) -> None:
    """Start with disabled defaults, schedule a recap when turned on, and reject incomplete setups."""
    recap = (await api_client.get("/api/state")).json()["recap"]
    assert recap == {
        "topics": [],
        "language": "English",
        "interval_seconds": 21_600,
        "enabled": False,
        "next_run_at": None,
    }
    assert (await api_client.post("/api/recap/send-now")).status_code == 400

    request = {"topics": [], "language": "Russian", "interval": "12h", "enabled": True}
    response = await api_client.put("/api/recap", json=request)
    assert response.status_code == 400
    assert (await api_client.put("/api/recap", json={**request, "interval": "2h"})).status_code == 422

    response = await api_client.put("/api/recap", json={**request, "topics": [" AI ", "AI", "Space"]})
    assert response.status_code == 200
    recap = response.json()
    assert (recap["topics"], recap["language"], recap["interval_seconds"]) == (["AI", "Space"], "Russian", 43_200)
    assert recap["enabled"]
    assert recap["next_run_at"] is not None
    assert (await api_client.post("/api/recap/send-now")).status_code == 200

    response = await api_client.put("/api/recap", json={**request, "topics": ["AI"], "enabled": False})
    assert response.json()["next_run_at"] is None
