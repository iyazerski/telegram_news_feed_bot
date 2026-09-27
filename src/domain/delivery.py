from dataclasses import dataclass
from typing import Literal

DeliveryAction = Literal["ack", "retry", "skip", "term"]


@dataclass(frozen=True)
class DeliveryResult:
    action: DeliveryAction
    error: str | None = None
