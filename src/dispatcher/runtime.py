import asyncio

from loguru import logger
from nats.aio.client import Client
from nats.aio.msg import Msg

from src.configs.configs import AppConfigs
from src.models.events import PostReferenceEvent
from src.services.dispatcher import TelegramForwardingService
from src.services.nats import NatsClientFactory


class DispatcherRuntime:
    def __init__(self, configs: AppConfigs, forwarding: TelegramForwardingService) -> None:
        """
        Create the NATS consumer runtime for discovered Telegram post references.
        """
        self.configs = configs
        self.forwarding = forwarding
        self.nats_factory = NatsClientFactory(configs.nats_url)

    async def run_forever(self) -> None:
        """
        Consume post reference events forever and dispatch repost delivery.
        """
        nats_client = await self.nats_factory.connect()
        await self.subscribe(nats_client)
        logger.info("Dispatcher connected to NATS")

        try:
            await asyncio.Event().wait()
        finally:
            await nats_client.close()

    async def subscribe(self, nats_client: Client) -> None:
        """
        Subscribe to discovered post references with a queue group.
        """
        await nats_client.subscribe(self.configs.nats_subject, queue=self.configs.nats_queue, cb=self.handle_message)

    async def handle_message(self, message: Msg) -> None:
        """
        Process one plain NATS post reference message.
        """
        event = PostReferenceEvent.model_validate_json(message.data)
        result = await self.forwarding.forward_event(event)

        if result.action == "ack":
            logger.info(f"Processed @{event.source_channel}/{event.message_id}")
            return

        if result.action == "term":
            logger.error(f"Telegram rejected @{event.source_channel}/{event.message_id}: {result.error}")
            return

        logger.warning(f"Delivery is not ready for @{event.source_channel}/{event.message_id}; waiting for rediscovery")
