from datetime import datetime, timedelta

from sqlalchemy.orm import Session

from src.domain.recaps import RecapSettings
from src.use_cases.manage_settings import SettingsService


class RecapScheduleService:
    def __init__(self, settings: SettingsService, default_interval_seconds: int) -> None:
        """
        Create the use case that stores AI news recap settings and their schedule.
        """
        self.settings = settings
        self.default_interval_seconds = default_interval_seconds

    def get_settings(self, session: Session) -> RecapSettings:
        """
        Return the stored AI news recap settings.
        """
        return self.settings.get_recap_settings(session, self.default_interval_seconds)

    def get_next_run_at(self, session: Session) -> datetime | None:
        """
        Return when the next recap runs, or None while recaps are off.
        """
        if not self.get_settings(session).enabled:
            return None
        return self.settings.get_recap_next_run_at(session)

    def save_settings(self, session: Session, settings: RecapSettings, now: datetime) -> None:
        """
        Store recap settings and reschedule so a new setup or a shorter interval applies right away.
        """
        if settings.enabled and not settings.topics:
            raise ValueError("Add at least one topic before turning AI recaps on.")

        previous = self.get_settings(session)
        next_run_at = self.settings.get_recap_next_run_at(session)
        if settings.enabled:
            # Turning recaps on sends the first one now, and a shorter interval never delays the next one.
            if not previous.enabled or next_run_at is None:
                next_run_at = now
            else:
                next_run_at = min(next_run_at, now + timedelta(seconds=settings.interval_seconds))
            self.settings.set_recap_next_run_at(session, next_run_at)
        self.settings.set_recap_settings(session, settings)

    def request_now(self, session: Session, now: datetime) -> None:
        """
        Make the recapper send a recap on its next check.
        """
        if not self.get_settings(session).enabled:
            raise ValueError("Turn AI recaps on before requesting one.")
        self.settings.set_recap_next_run_at(session, now)
