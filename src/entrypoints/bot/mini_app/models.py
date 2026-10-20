from datetime import datetime
from typing import Annotated, Literal

from pydantic import BaseModel, Field, StringConstraints

from src.domain.recaps import ThinkingLevel

RecapTopic = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=80)]


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


class ChannelPageResponse(BaseModel):
    channels: list[ChannelResponse]
    next_cursor: str | None


class SettingsResponse(BaseModel):
    poll_interval_seconds: int


class RecapResponse(BaseModel):
    topics: list[str]
    language: str
    interval_seconds: int
    enabled: bool
    model: str
    thinking: ThinkingLevel
    next_run_at: datetime | None
    available_models: list[str]


class AppStateResponse(ChannelPageResponse, SettingsResponse):
    recap: RecapResponse


class AddChannelRequest(BaseModel):
    username_or_url: str = Field(min_length=1)


class PollIntervalRequest(BaseModel):
    interval: Literal["1m", "5m", "15m", "1h"]


class RecapSettingsRequest(BaseModel):
    topics: list[RecapTopic] = Field(max_length=10)
    language: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=40)]
    interval: Literal["3h", "6h", "12h", "24h"]
    enabled: bool
    model: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=120)]
    thinking: ThinkingLevel
