from sqlalchemy import select
from sqlalchemy.orm import Session

from src.models.database import AppSetting

DESTINATION_CHAT_ID_KEY = "destination_chat_id"
POLL_INTERVAL_SECONDS_KEY = "poll_interval_seconds"


class SettingsService:
    def get_value(self, session: Session, key: str) -> str | None:
        """
        Return a configured setting value by key.
        """
        return session.scalar(select(AppSetting.value).where(AppSetting.key == key))

    def set_value(self, session: Session, key: str, value: str) -> None:
        """
        Create or update a configured setting value by key.
        """
        setting = session.get(AppSetting, key)

        # Upsert by primary key while preserving the single-row setting shape.
        if setting is None:
            session.add(AppSetting(key=key, value=value))
        else:
            setting.value = value

    def get_destination_chat_id(self, session: Session) -> str | None:
        """
        Return the stored bot-chat destination ID.
        """
        return self.get_value(session, DESTINATION_CHAT_ID_KEY)

    def set_destination_chat_id(self, session: Session, destination: str) -> None:
        """
        Store the bot-chat destination ID.
        """
        self.set_value(session, DESTINATION_CHAT_ID_KEY, destination)

    def get_poll_interval_seconds(self, session: Session, default_seconds: int) -> int:
        """
        Return the configured poll interval in seconds.
        """
        value = self.get_value(session, POLL_INTERVAL_SECONDS_KEY)
        if value is None:
            return default_seconds
        return int(value)

    def set_poll_interval_seconds(self, session: Session, seconds: int) -> None:
        """
        Store the poll interval in seconds.
        """
        self.set_value(session, POLL_INTERVAL_SECONDS_KEY, str(seconds))
