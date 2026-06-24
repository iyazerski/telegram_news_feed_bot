from src.configs.configs import AppConfigs
from src.orm.database import Database
from src.services.channels import ChannelService
from src.services.settings import SettingsService
from src.utils.telegram import parse_duration_seconds


class BotCommandHandler:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the command handler for Telegram bot text commands.
        """
        self.configs = configs
        self.db = db
        self.settings = settings
        self.channels = ChannelService()

    def handle_text(self, text: str) -> str:
        """
        Execute a supported bot command and return a response message.
        """
        try:
            return self.handle_command(text)
        except ValueError as exc:
            return str(exc)

    def handle_command(self, text: str) -> str:
        """
        Execute a supported bot command and return a response message.
        """
        command, args = self.split_command(text)

        if command == "/start":
            return self.help_text()
        if command == "/add_channel":
            return self.add_channel(args)
        if command == "/remove_channel":
            return self.remove_channel(args)
        if command == "/list_channels":
            return self.list_channels()
        if command == "/set_interval":
            return self.set_interval(args)
        if command == "/status":
            return self.status()

        return self.help_text()

    def split_command(self, text: str) -> tuple[str, str]:
        """
        Split Telegram command text into command and argument text.
        """
        parts = text.strip().split(maxsplit=1)
        command = parts[0].split("@", maxsplit=1)[0].lower()
        args = parts[1].strip() if len(parts) == 2 else ""
        return command, args

    def add_channel(self, args: str) -> str:
        """
        Add a source channel from command arguments.
        """
        if not args:
            return "Usage: /add_channel <channel_username_or_url>"

        with self.db.create_session() as session:
            channel = self.channels.add_channel(session, args)
            session.commit()

        return f"Added @{channel.username}."

    def remove_channel(self, args: str) -> str:
        """
        Remove a source channel from command arguments.
        """
        if not args:
            return "Usage: /remove_channel <channel_username>"

        with self.db.create_session() as session:
            channel = self.channels.remove_channel(session, args)
            session.commit()

        return f"Removed @{channel.username}."

    def start(self, chat_id: int | str) -> str:
        """
        Store the current bot chat as the destination and return supported commands.
        """
        with self.db.create_session() as session:
            self.settings.set_destination_chat_id(session, str(chat_id))
            session.commit()

        return "\n".join(["This chat is now the feed destination.", "", self.help_text()])

    def list_channels(self) -> str:
        """
        Return a formatted list of active source channels.
        """
        with self.db.create_session() as session:
            channels = self.channels.list_active_channels(session)

        if not channels:
            return "No source channels configured."

        lines = ["Source channels:"]
        lines.extend(
            f"- @{channel.username} (last committed: {channel.last_committed_message_id})" for channel in channels
        )
        return "\n".join(lines)

    def set_interval(self, args: str) -> str:
        """
        Store the polling interval from command arguments.
        """
        if not args:
            return "Usage: /set_interval <seconds_or_5m_or_1h>"

        seconds = parse_duration_seconds(args)
        with self.db.create_session() as session:
            self.settings.set_poll_interval_seconds(session, seconds)
            session.commit()

        return f"Polling interval set to {seconds} seconds."

    def status(self) -> str:
        """
        Return current destination, interval, and source count.
        """
        with self.db.create_session() as session:
            channels = self.channels.list_active_channels(session)
            destination = self.settings.get_destination_chat_id(session)
            interval = self.settings.get_poll_interval_seconds(session, self.configs.default_poll_interval_seconds)

        destination_text = "this bot chat" if destination is not None else "not initialized; send /start"
        return "\n".join(
            [
                f"Destination: {destination_text}",
                f"Polling interval: {interval} seconds",
                f"Active channels: {len(channels)}",
            ]
        )

    def help_text(self) -> str:
        """
        Return supported command usage text.
        """
        return "\n".join(
            [
                "Commands:",
                "/add_channel <channel_username_or_url>",
                "/remove_channel <channel_username>",
                "/list_channels",
                "/set_interval <seconds_or_5m_or_1h>",
                "/status",
            ]
        )
