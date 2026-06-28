from typing import Literal

from pydantic import BaseModel, Field


class TelegramMiniAppUser(BaseModel):
    id: int
    first_name: str
    last_name: str | None = None
    username: str | None = None
    language_code: str | None = None


class TelegramMiniAppSession(BaseModel):
    user: TelegramMiniAppUser
    auth_date: int


class ChannelResponse(BaseModel):
    username: str
    url: str


class AppStateResponse(BaseModel):
    poll_interval_seconds: int
    channels: list[ChannelResponse]


class AddChannelRequest(BaseModel):
    username_or_url: str = Field(min_length=1)


class PollIntervalRequest(BaseModel):
    interval: Literal["1m", "5m", "15m", "1h"]
