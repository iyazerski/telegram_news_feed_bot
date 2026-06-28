from src.configs.configs import AppConfigs
from src.orm.database import Database
from src.services.settings import SettingsService

HELP_TEXT = """
Your personal Telegram news feed is ready 📰
Collect posts from public channels in one place, tune your sources, and keep the updates flowing automatically ✨
Open the menu to manage your feed👇
""".strip()


class BotCommandHandler:
    def __init__(self, configs: AppConfigs, db: Database, settings: SettingsService) -> None:
        """
        Create the command handler for Telegram bot text commands.
        """
        self.configs = configs
        self.db = db
        self.settings = settings

    def start(self, chat_id: int | str) -> str:
        """
        Store the current bot chat as the destination and return help text.
        """
        with self.db.create_session() as session:
            self.settings.set_destination_chat_id(session, str(chat_id))
            session.commit()

        return HELP_TEXT
