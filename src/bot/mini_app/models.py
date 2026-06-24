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
    last_committed_message_id: int
    url: str


class AppStateResponse(BaseModel):
    destination_chat_id: str | None
    poll_interval_seconds: int
    channels: list[ChannelResponse]


class AddChannelRequest(BaseModel):
    username_or_url: str = Field(min_length=1)


class PollIntervalRequest(BaseModel):
    interval: str = Field(min_length=1)
