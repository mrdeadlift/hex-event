import { createClient } from '../../src/index.js';

async function main(): Promise<void> {
  const client = createClient();

  client.onError((error) => {
    // eslint-disable-next-line no-console
    console.error('[example] client error', error);
  });

  client.on('kill', (event) => {
    const { summonerName, team, slot } = event.payload.player;
    // eslint-disable-next-line no-console
    console.log(`[kill] ${summonerName} (${team} ${slot})`);
  });

  client.on('phaseChange', (event) => {
    // eslint-disable-next-line no-console
    console.log(`[phase] ${event.payload.phase}`);
  });

  await client.connect();
  // eslint-disable-next-line no-console
  console.log('Connected to levents daemon; waiting for events...');

  // Manual emit to demonstrate dispatching prior to a running daemon.
  client.emit({
    kind: 'kill',
    ts: Date.now(),
    payload: {
      payloadKind: 'player',
      player: {
        summonerName: 'Example',
        team: 'order',
        slot: 0
      }
    }
  });

  process.on('SIGINT', async () => {
    // eslint-disable-next-line no-console
    console.log('Shutting down...');
    await client.disconnect();
  });
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exitCode = 1;
});
