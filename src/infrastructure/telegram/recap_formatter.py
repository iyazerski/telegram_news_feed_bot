import html

from src.domain.recaps import RecapStory
from src.infrastructure.telegram.message_formatter import TEXT_MESSAGE_LIMIT

EMPTY_RECAP_TEXT = "No important new stories for your topics since the last recap."


class NewsRecapFormatter:
    def build_message(self, stories: list[RecapStory], topics: list[str]) -> str:
        """
        Build one Telegram HTML recap message, dropping trailing stories that exceed the length limit.
        """
        header = f"🗞 <b>News recap</b>\n<i>{html.escape(' · '.join(topics), quote=False)}</i>"
        if not stories:
            return f"{header}\n\n{EMPTY_RECAP_TEXT}"

        blocks = [self.build_story(story) for story in stories]
        # Telegram counts visible text, which is never longer than the HTML, so this bound is safe.
        while len(blocks) > 1 and len(self.join(header, blocks)) > TEXT_MESSAGE_LIMIT:
            blocks.pop()
        return self.join(header, blocks)

    def build_story(self, story: RecapStory) -> str:
        """
        Build one escaped recap story block with its source link.
        """
        return (
            f"{html.escape(story.emoji, quote=False)} <b>{html.escape(story.title, quote=False)}</b>\n"
            f"{html.escape(story.summary, quote=False)}\n"
            f'🔗 <a href="{html.escape(story.url)}">{html.escape(story.source_name, quote=False)}</a>'
        )

    def join(self, header: str, blocks: list[str]) -> str:
        """
        Join the recap header and story blocks with blank lines.
        """
        return "\n\n".join([header, *blocks])
