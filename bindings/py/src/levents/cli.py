"""CLI entry points for levents-py."""

from __future__ import annotations

import json
from typing import Optional

import typer

from .client import ClientConfig, LeventsClient
from .models import Event, EventKind

app = typer.Typer(help="Interact with the levents daemon from Python.")


@app.command()
def watch(event: Optional[EventKind] = typer.Option(None, help="Filter to a specific event kind.")) -> None:
    """Tail events from the daemon. Currently streams in-memory stubs."""

    client = LeventsClient(ClientConfig())
    client.connect()

    def printer(evt: Event) -> None:
        if event is None or evt.kind == event:
            typer.echo(json.dumps(evt.model_dump(mode="json")))

    target_kind = event or EventKind.HEARTBEAT
    handle = client.subscribe(target_kind, printer)

    # Emit a bootstrap heartbeat so the CLI prints something useful when the daemon is offline.
    client.emit({
        "kind": target_kind.value,
        "ts": 0,
        "payload": {"payloadKind": "heartbeat", "seq": 0},
    })

    handle.close()
    client.close()
