import json
from datetime import datetime

from sqlalchemy import select
from sqlalchemy.orm import Session

from src.domain.recaps import RecapSettings
from src.infrastructure.database.models import AppSetting

DESTINATION_CHAT_ID_KEY = "destination_chat_id"
POLL_INTERVAL_SECONDS_KEY = "poll_interval_seconds"
RECAP_TOPICS_KEY = "recap_topics"
RECAP_LANGUAGE_KEY = "recap_language"
RECAP_INTERVAL_SECONDS_KEY = "recap_interval_seconds"
RECAP_ENABLED_KEY = "recap_enabled"
RECAP_NEXT_RUN_AT_KEY = "recap_next_run_at"
DEFAULT_RECAP_LANGUAGE = "English"


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

    def get_recap_settings(self, session: Session, default_interval_seconds: int) -> RecapSettings:
        """
        Return the AI news recap settings, or disabled defaults before the first save.
        """
        topics = self.get_value(session, RECAP_TOPICS_KEY)
        interval_seconds = self.get_value(session, RECAP_INTERVAL_SECONDS_KEY)
        return RecapSettings(
            topics=json.loads(topics) if topics is not None else [],
            language=self.get_value(session, RECAP_LANGUAGE_KEY) or DEFAULT_RECAP_LANGUAGE,
            interval_seconds=int(interval_seconds) if interval_seconds is not None else default_interval_seconds,
            enabled=self.get_value(session, RECAP_ENABLED_KEY) == "true",
        )

    def set_recap_settings(self, session: Session, settings: RecapSettings) -> None:
        """
        Store the AI news recap settings.
        """
        self.set_value(session, RECAP_TOPICS_KEY, json.dumps(settings.topics, ensure_ascii=False))
        self.set_value(session, RECAP_LANGUAGE_KEY, settings.language)
        self.set_value(session, RECAP_INTERVAL_SECONDS_KEY, str(settings.interval_seconds))
        self.set_value(session, RECAP_ENABLED_KEY, "true" if settings.enabled else "false")

    def get_recap_next_run_at(self, session: Session) -> datetime | None:
        """
        Return when the next AI news recap is due, or None when none is scheduled.
        """
        value = self.get_value(session, RECAP_NEXT_RUN_AT_KEY)
        return datetime.fromisoformat(value) if value else None

    def set_recap_next_run_at(self, session: Session, run_at: datetime) -> None:
        """
        Store when the next AI news recap is due.
        """
        self.set_value(session, RECAP_NEXT_RUN_AT_KEY, run_at.isoformat())
