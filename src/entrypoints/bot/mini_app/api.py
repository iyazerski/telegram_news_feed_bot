from typing import Annotated

from fastapi import APIRouter, Depends, Header, HTTPException, Query, Response
from sqlalchemy.orm import Session

from src.config.configs import AppConfigs
from src.entrypoints.bot.mini_app.auth import validate_mini_app_init_data
from src.entrypoints.bot.mini_app.models import (
    AddChannelRequest,
    AppStateResponse,
    ChannelPageResponse,
    ChannelResponse,
    PollIntervalRequest,
    SettingsResponse,
    TelegramMiniAppSession,
)
from src.infrastructure.database.orm import Database
from src.use_cases.manage_channels import ChannelService
from src.use_cases.manage_settings import SettingsService

POLL_INTERVAL_SECONDS = {
    "1m": 60,
    "5m": 300,
    "15m": 900,
    "1h": 3_600,
}
CHANNEL_PAGE_SIZE = 100


def create_mini_app_api_router(configs: AppConfigs, db: Database, settings: SettingsService) -> APIRouter:
    """Create authenticated Mini App API routes for channel and settings management."""
    router = APIRouter(prefix="/api")
    channels = ChannelService()

    def build_channel_page(database_session: Session, after: str) -> ChannelPageResponse:
        """Read a bounded channel page and expose a cursor when more rows exist."""
        rows = channels.list_active_channels(database_session, after=after, limit=CHANNEL_PAGE_SIZE + 1)
        page = rows[:CHANNEL_PAGE_SIZE]
        return ChannelPageResponse(
            channels=[
                ChannelResponse(username=channel.username, url=f"https://t.me/{channel.username}") for channel in page
            ],
            next_cursor=page[-1].username if len(rows) > CHANNEL_PAGE_SIZE else None,
        )

    def authenticate_admin(init_data: Annotated[str, Header(alias="X-Telegram-Init-Data")]) -> TelegramMiniAppSession:
        """Authenticate one Mini App API request as the configured Telegram admin."""
        try:
            session = validate_mini_app_init_data(init_data, configs.bot_token, configs.mini_app_auth_max_age_seconds)
        except ValueError as exc:
            raise HTTPException(status_code=401, detail=str(exc)) from exc

        if configs.admin_user_id and str(session.user.id) != configs.admin_user_id:
            raise HTTPException(status_code=403, detail="This bot is restricted to its configured admin.")
        return session

    @router.get("/state", response_model=AppStateResponse)
    def get_state(_session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)]) -> AppStateResponse:
        """Return settings and the first page of channel subscriptions."""
        with db.create_session() as database_session:
            page = build_channel_page(database_session, "")
            return AppStateResponse(
                poll_interval_seconds=settings.get_poll_interval_seconds(
                    database_session,
                    configs.default_poll_interval_seconds,
                ),
                channels=page.channels,
                next_cursor=page.next_cursor,
            )

    @router.get("/channels", response_model=ChannelPageResponse)
    def get_channels(
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
        after: Annotated[str, Query(max_length=120)] = "",
    ) -> ChannelPageResponse:
        """Return the next page of active channel subscriptions."""
        with db.create_session() as database_session:
            return build_channel_page(database_session, after)

    @router.post("/channels", response_model=ChannelResponse)
    def add_channel(
        request: AddChannelRequest,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> ChannelResponse:
        """Add or reactivate a source channel and return that subscription."""
        with db.create_session() as database_session:
            try:
                channel = channels.add_channel(database_session, request.username_or_url)
            except ValueError as exc:
                raise HTTPException(status_code=400, detail=str(exc)) from exc
            database_session.commit()
            return ChannelResponse(username=channel.username, url=f"https://t.me/{channel.username}")

    @router.delete("/channels/{username}", status_code=204)
    def remove_channel(
        username: str,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> Response:
        """Deactivate a source channel without reloading other subscriptions."""
        with db.create_session() as database_session:
            try:
                channels.remove_channel(database_session, username)
            except ValueError as exc:
                raise HTTPException(status_code=400, detail=str(exc)) from exc
            database_session.commit()
        return Response(status_code=204)

    @router.patch("/settings/poll-interval", response_model=SettingsResponse)
    def update_poll_interval(
        request: PollIntervalRequest,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> SettingsResponse:
        """Update the polling interval and return its stored value."""
        seconds = POLL_INTERVAL_SECONDS[request.interval]
        with db.create_session() as database_session:
            settings.set_poll_interval_seconds(database_session, seconds)
            database_session.commit()
        return SettingsResponse(poll_interval_seconds=seconds)

    return router
