import { createClient } from "../../src/index.js";

async function main(): Promise<void> {
  const client = createClient();

  client.onError((error) => {
    // eslint-disable-next-line no-console
    console.error("[example] client error", error);
  });

  client.on("kill", (event) => {
    if (event.payload.payloadKind === "player") {
      const { summonerName, team, slot } = event.payload.player;
      // eslint-disable-next-line no-console
      console.log(`[kill] ${summonerName} (${team} ${slot})`);
    }
  });

  client.on("itemAdded", (event) => {
    if (event.payload.payloadKind === "playerItem") {
      const { summonerName, team, slot } = event.payload.player;
      const { itemId, itemName } = event.payload;
      // eslint-disable-next-line no-console
      console.log(
        `[itemAdded] ${summonerName} (${team} ${slot}) -> ${itemName ?? "#"}${
          itemName ? "" : itemId
        }`
      );
    }
  });

  client.on("phaseChange", (event) => {
    if (event.payload.payloadKind === "phase") {
      // eslint-disable-next-line no-console
      console.log(`[phase] ${event.payload.phase}`);
    }
  });

  await client.connect();
  // eslint-disable-next-line no-console
  console.log("Connected to levents daemon; waiting for events...");

  // Manual emit to demonstrate dispatching prior to a running daemon.
  client.emit({
    kind: "kill",
    ts: Date.now(),
    payload: {
      payloadKind: "player" as const,
      player: {
        summonerName: "Example",
        team: "order" as const,
        slot: 0,
      },
    },
  });

  process.on("SIGINT", async () => {
    // eslint-disable-next-line no-console
    console.log("Shutting down...");
    await client.disconnect();
  });
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error("Fatal error:", error);
  process.exitCode = 1;
});
