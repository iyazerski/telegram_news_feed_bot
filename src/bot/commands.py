from src.configs.configs import AppConfigs
from src.orm.database import Database
from src.services.settings import SettingsService


class BotCommandHandler:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the command handler for Telegram bot text commands.
        """
        self.configs = configs
        self.db = db
        self.settings = settings

    def handle_text(self, _text: str) -> str:
        """
        Return the default bot introduction for any chat text.
        """
        return self.help_text()

    def start(self, chat_id: int | str) -> str:
        """
        Store the current bot chat as the destination and return supported commands.
        """
        with self.db.create_session() as session:
            self.settings.set_destination_chat_id(session, str(chat_id))
            session.commit()

        return "\n".join(["This chat is now the feed destination.", "", self.help_text()])

    def help_text(self) -> str:
        """
        Return a short product introduction for chat users.
        """
        return "\n".join(
            [
                "Your personal Telegram news feed is ready 📰",
                "Collect posts from public channels in one place, tune your sources, and keep the updates flowing automatically ✨",
                "Open the menu to manage your feed👇",
            ]
        )
