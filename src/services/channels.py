from sqlalchemy import select
from sqlalchemy.orm import Session

from src.models.database import SourceChannel
from src.utils.telegram import normalize_channel_username


class ChannelService:
    def add_channel(self, session: Session, username_or_url: str) -> SourceChannel:
        """
        Add or reactivate a source channel subscription.
        """
        username = normalize_channel_username(username_or_url)
        channel = session.scalar(select(SourceChannel).where(SourceChannel.username == username))

        # Reactivate existing rows so historical state remains attached to the channel.
        if channel is None:
            channel = SourceChannel(username=username)
            session.add(channel)
            session.flush()
        else:
            channel.active = True

        return channel

    def remove_channel(self, session: Session, username_or_url: str) -> SourceChannel:
        """
        Deactivate a source channel subscription.
        """
        username = normalize_channel_username(username_or_url)
        channel = session.scalar(select(SourceChannel).where(SourceChannel.username == username))
        if channel is None:
            raise ValueError(f"Channel @{username} is not configured")

        channel.active = False
        return channel

    def list_active_channels(self, session: Session) -> list[SourceChannel]:
        """
        Return active source channel subscriptions ordered by username.
        """
        return list(session.scalars(select(SourceChannel).where(SourceChannel.active).order_by(SourceChannel.username)))

    def get_active_channel(self, session: Session, username: str) -> SourceChannel:
        """
        Return an active source channel by normalized username.
        """
        channel = session.scalar(select(SourceChannel).where(SourceChannel.username == username, SourceChannel.active))
        if channel is None:
            raise ValueError(f"Active channel @{username} is not configured")
        return channel
