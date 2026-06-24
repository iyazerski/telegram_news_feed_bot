from typing import Annotated

from fastapi import APIRouter, Depends, Header, HTTPException

from src.bot.mini_app.auth import validate_mini_app_init_data
from src.bot.mini_app.models import (
    AddChannelRequest,
    AppStateResponse,
    ChannelResponse,
    PollIntervalRequest,
    TelegramMiniAppSession,
)
from src.configs.configs import AppConfigs
from src.orm.database import Database
from src.services.channels import ChannelService
from src.services.settings import SettingsService

POLL_INTERVAL_SECONDS = {
    "1m": 60,
    "5m": 300,
    "15m": 900,
    "1h": 3_600,
}


def create_mini_app_api_router(configs: AppConfigs, db: Database, settings: SettingsService) -> APIRouter:
    """
    Create authenticated Mini App API routes for channel and settings management.
    """
    router = APIRouter(prefix="/api")
    channels = ChannelService()

    def authenticate_admin(init_data: Annotated[str, Header(alias="X-Telegram-Init-Data")]) -> TelegramMiniAppSession:
        """
        Authenticate one Mini App API request as the configured Telegram admin.
        """
        try:
            session = validate_mini_app_init_data(init_data, configs.bot_token, configs.mini_app_auth_max_age_seconds)
        except ValueError as exc:
            raise HTTPException(status_code=401, detail=str(exc)) from exc

        if configs.admin_user_id and str(session.user.id) != configs.admin_user_id:
            raise HTTPException(status_code=403, detail="This bot is restricted to its configured admin.")

        return session

    @router.get("/state", response_model=AppStateResponse)
    def get_state(_session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)]) -> AppStateResponse:
        """
        Return the current Mini App dashboard state.
        """
        with db.create_session() as database_session:
            active_channels = channels.list_active_channels(database_session)
            poll_interval_seconds = settings.get_poll_interval_seconds(
                database_session,
                configs.default_poll_interval_seconds,
            )

        return AppStateResponse(
            poll_interval_seconds=poll_interval_seconds,
            channels=[
                ChannelResponse(
                    username=channel.username,
                    url=f"https://t.me/{channel.username}",
                )
                for channel in active_channels
            ],
        )

    @router.post("/channels", response_model=AppStateResponse)
    def add_channel(
        request: AddChannelRequest,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> AppStateResponse:
        """
        Add or reactivate a source channel and return the updated dashboard state.
        """
        with db.create_session() as database_session:
            try:
                channels.add_channel(database_session, request.username_or_url)
            except ValueError as exc:
                raise HTTPException(status_code=400, detail=str(exc)) from exc
            database_session.commit()

        return get_state(_session)

    @router.delete("/channels/{username}", response_model=AppStateResponse)
    def remove_channel(
        username: str,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> AppStateResponse:
        """
        Deactivate a source channel and return the updated dashboard state.
        """
        with db.create_session() as database_session:
            try:
                channels.remove_channel(database_session, username)
            except ValueError as exc:
                raise HTTPException(status_code=400, detail=str(exc)) from exc
            database_session.commit()

        return get_state(_session)

    @router.patch("/settings/poll-interval", response_model=AppStateResponse)
    def update_poll_interval(
        request: PollIntervalRequest,
        _session: Annotated[TelegramMiniAppSession, Depends(authenticate_admin)],
    ) -> AppStateResponse:
        """
        Update the polling interval from a preset and return the updated dashboard state.
        """
        with db.create_session() as database_session:
            settings.set_poll_interval_seconds(database_session, POLL_INTERVAL_SECONDS[request.interval])
            database_session.commit()

        return get_state(_session)

    return router
