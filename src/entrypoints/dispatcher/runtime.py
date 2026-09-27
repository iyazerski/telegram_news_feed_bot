import asyncio

import nats
from loguru import logger
from nats.aio.client import Client
from nats.aio.msg import Msg

from src.config.configs import AppConfigs
from src.infrastructure.messaging.events import PostReferenceBatchEvent, PostReferenceEvent
from src.use_cases.deliver_posts import TelegramForwardingService


class DispatcherRuntime:
    def __init__(self, configs: AppConfigs, forwarding: TelegramForwardingService) -> None:
        """
        Create the NATS consumer runtime for discovered Telegram post references.
        """
        self.configs = configs
        self.forwarding = forwarding

    async def run_forever(self) -> None:
        """
        Consume post reference events forever and dispatch repost delivery.
        """
        nats_client = await nats.connect(self.configs.nats_url)
        await self.subscribe(nats_client)
        logger.info("Dispatcher connected to NATS")

        try:
            await asyncio.Event().wait()
        finally:
            await nats_client.close()
            await self.forwarding.close()

    async def subscribe(self, nats_client: Client) -> None:
        """
        Subscribe to discovered post references with a queue group.
        """
        await nats_client.subscribe(self.configs.nats_subject, queue=self.configs.nats_queue, cb=self.handle_message)

    async def handle_message(self, message: Msg) -> None:
        """
        Process one plain NATS post reference message.
        """
        batch = PostReferenceBatchEvent.model_validate_json(message.data)
        for event in batch.posts:
            should_continue = await self.handle_event(event)
            if not should_continue:
                break

    async def handle_event(self, event: PostReferenceEvent) -> bool:
        """
        Process one ordered post and return whether later posts may proceed.
        """
        result = await self.forwarding.forward_event(event)

        if result.action == "ack":
            logger.info(f"Processed @{event.source_channel}/{event.message_id}")
            return True

        if result.action == "term":
            logger.error(f"Telegram rejected @{event.source_channel}/{event.message_id}: {result.error}")
            return True

        # A batch holds one channel, so later posts of a removed channel are skipped too.
        if result.action == "skip":
            logger.info(f"Skipped @{event.source_channel}/{event.message_id}: channel is no longer active")
            return False

        logger.warning(f"Delivery is not ready for @{event.source_channel}/{event.message_id}; waiting for rediscovery")
        return False
