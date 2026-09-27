from datetime import UTC, datetime

import pytest
from nats.aio.client import Client
from nats.aio.msg import Msg

from src.config.configs import AppConfigs
from src.domain.delivery import DeliveryResult
from src.entrypoints.dispatcher.runtime import DispatcherRuntime
from src.infrastructure.messaging.events import PostReferenceBatchEvent, PostReferenceEvent
from src.use_cases.deliver_posts import TelegramForwardingService

pytestmark = pytest.mark.unit


class RecordingForwardingService(TelegramForwardingService):
    def __init__(self, results: list[DeliveryResult]) -> None:
        """
        Create a forwarding service that returns configured delivery results.
        """
        self.results = results
        self.forwarded_message_ids: list[int] = []

    async def forward_event(self, event: PostReferenceEvent) -> DeliveryResult:
        """
        Record one forwarding attempt and return its configured result.
        """
        self.forwarded_message_ids.append(event.message_id)
        return self.results.pop(0)

    async def close(self) -> None:
        """
        Close the fake forwarding service.
        """


def create_event(message_id: int) -> PostReferenceEvent:
    """
    Create one dispatcher event with a configurable message ID.
    """
    return PostReferenceEvent(
        source_channel="example",
        channel_display_name="Example",
        message_id=message_id,
        text_html="Post",
        media_urls=[],
        post_url=f"https://t.me/example/{message_id}",
        discovered_at=datetime.now(UTC),
    )


@pytest.mark.asyncio
async def test_handle_message_stops_batch_after_retry() -> None:
    """
    Verify a failed earlier post prevents a later cursor-advancing delivery.
    """
    forwarding = RecordingForwardingService([DeliveryResult(action="retry"), DeliveryResult(action="ack")])
    runtime = DispatcherRuntime(AppConfigs(), forwarding)
    batch = PostReferenceBatchEvent(posts=[create_event(42), create_event(43)])
    message = Msg(Client(), data=batch.model_dump_json().encode())

    await runtime.handle_message(message)

    assert forwarding.forwarded_message_ids == [42]


@pytest.mark.asyncio
async def test_handle_message_stops_batch_after_removed_channel() -> None:
    """
    Verify posts of a removed channel stop the batch without raising.
    """
    forwarding = RecordingForwardingService([DeliveryResult(action="skip"), DeliveryResult(action="ack")])
    runtime = DispatcherRuntime(AppConfigs(), forwarding)
    batch = PostReferenceBatchEvent(posts=[create_event(42), create_event(43)])
    message = Msg(Client(), data=batch.model_dump_json().encode())

    await runtime.handle_message(message)

    assert forwarding.forwarded_message_ids == [42]
