from dataclasses import dataclass


@dataclass(frozen=True)
class DeliveryResult:
    action: str
    error: str | None = None
