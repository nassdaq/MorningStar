// Entry point: `npm start`.
//
// `start` is also what the integration test calls, with port 0.

import http from 'node:http';
import { pathToFileURL } from 'node:url';

import { createApp } from './app.js';
import { loadConfig, loadEnvFile, secrets } from './config.js';
import { createDb } from './db.js';
import { createLogger } from './log.js';
import { createModel } from './model.js';
import { createVoice } from './voice.js';

/**
 * Migrates the database, reads the voice list and starts listening.
 * Returns `{port, db, close}`; `close` finishes requests in flight, then
 * closes the pool.
 */
export async function start(config, { quiet = false } = {}) {
  const log = createLogger({ secrets: secrets(config), quiet });
  if (!config.databaseUrl) throw new Error('DATABASE_URL is not set (see .env.example)');

  const db = createDb(config, log);
  await db.migrate();
  await db.warm();
  const model = createModel(config, log);
  const voice = createVoice(config, log);
  await voice.init();
  if (!model.enabled) log.warn('DEEPSEEK_API_KEY is not set: calls cannot be played or scored');

  const app = createApp({ config, db, model, voice, log });
  const server = http.createServer((req, res) => {
    app.handle(req, res);
  });
  server.requestTimeout = 60000; // a slow upload cannot hold a socket for ever
  server.headersTimeout = 15000;
  server.keepAliveTimeout = 5000;

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(config.port, config.host, resolve);
  });
  const { port } = server.address();
  log.info(
    `listening on ${config.host}:${port} model=${model.enabled ? 'on' : 'off'} voice=${voice.enabled ? 'on' : 'off'} token=${config.appToken ? 'required' : 'not set'}`,
  );

  let closing = null;
  function close() {
    closing ??= (async () => {
      const closed = new Promise((resolve) => server.close(resolve));
      server.closeIdleConnections();
      // Requests in flight get ten seconds; an upstream call is at most 2 x 15 s,
      // but nobody waits that long for a shutdown.
      const timer = setTimeout(() => server.closeAllConnections(), 10000);
      await closed;
      clearTimeout(timer);
      app.close();
      await db.close();
    })();
    return closing;
  }

  return { port, db, close, log };
}

async function main() {
  loadEnvFile();
  const config = loadConfig();
  const fallback = createLogger({ secrets: secrets(config) });
  let running;
  try {
    running = await start(config);
  } catch (err) {
    fallback.error('could not start', err);
    process.exit(1);
  }
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
      running.log.info(`${signal} received, shutting down`);
      running.close().then(
        () => process.exit(0),
        (err) => {
          running.log.error('shutdown failed', err);
          process.exit(1);
        },
      );
    });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
