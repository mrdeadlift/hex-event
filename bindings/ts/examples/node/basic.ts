import { createClient } from '../../src/index.js';

async function main(): Promise<void> {
  const client = createClient();
  await client.connect();

  client.on('kill', (event) => {
    console.log('kill event', event);
  });

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
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
