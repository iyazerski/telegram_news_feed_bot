from dataclasses import dataclass


@dataclass(frozen=True)
class RecapSettings:
    topics: list[str]
    language: str
    interval_seconds: int
    enabled: bool
