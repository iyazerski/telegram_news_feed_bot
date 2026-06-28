import nats
from nats.aio.client import Client


class NatsClientFactory:
    def __init__(self, nats_url: str) -> None:
        """
        Store NATS connection configuration.
        """
        self.nats_url = nats_url

    async def connect(self) -> Client:
        """
        Connect to NATS as a lightweight message bus.
        """
        return await nats.connect(self.nats_url)
