"""Public client API exposed to Python integrations."""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from typing import Callable, DefaultDict, Dict, Iterable, Optional

from .models import Event, EventKind

EventHandler = Callable[[Event], None]


@dataclass
class ClientConfig:
    live_enabled: bool = True
    lcu_enabled: bool = True
    endpoint: str = "127.0.0.1:50051"


class SubscribeHandle:
    """Handle returned by :meth:`LeventsClient.subscribe` for unsubscription."""

    def __init__(self, client: 'LeventsClient', kind: EventKind, handler: EventHandler) -> None:
        self._client = client
        self._kind = kind
        self._handler = handler

    def close(self) -> None:
        self._client.unsubscribe(self._kind, self._handler)


class LeventsClient:
    """In-memory event dispatcher that mirrors the future gRPC API."""

    def __init__(self, config: Optional[ClientConfig] = None) -> None:
        self._config = config or ClientConfig()
        self._handlers: DefaultDict[EventKind, list[EventHandler]] = defaultdict(list)

    @property
    def config(self) -> ClientConfig:
        return self._config

    def connect(self) -> None:
        """Placeholder for establishing a gRPC channel."""

    def subscribe(self, event: EventKind | str, handler: EventHandler) -> SubscribeHandle:
        kind = EventKind(event)
        self._handlers[kind].append(handler)
        return SubscribeHandle(self, kind, handler)

    def unsubscribe(self, event: EventKind | str, handler: EventHandler) -> None:
        kind = EventKind(event)
        handlers = self._handlers[kind]
        try:
            handlers.remove(handler)
        except ValueError:
            pass

    def emit(self, event: Event | Dict) -> None:
        payload = event if isinstance(event, Event) else Event.model_validate(event)
        for handler in list(self._handlers[payload.kind]):
            handler(payload)

    def close(self) -> None:
        self._handlers.clear()

    def registered_kinds(self) -> Iterable[EventKind]:
        return self._handlers.keys()
