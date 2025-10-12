from levents.client import ClientConfig, LeventsClient
from levents.models import EventKind


def test_emit_dispatches_to_handler():
    client = LeventsClient(ClientConfig())
    captured: list[str] = []

    def handler(event):
        captured.append(event.kind.value)

    handle = client.subscribe(EventKind.KILL, handler)
    client.emit({
        "kind": EventKind.KILL.value,
        "ts": 0,
        "payload": {
            "payloadKind": "player",
            "player": {
                "summonerName": "Example",
                "team": "order",
                "slot": 0,
            },
        },
    })

    assert captured == [EventKind.KILL.value]
    handle.close()
    client.close()
