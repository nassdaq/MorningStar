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
 * Keeps the process alive through an error nothing caught: a rejected
 * promise nobody awaited, or an exception thrown from a timer or an event
 * (a database connection that drops while a query holds it emits one).
 * Node's own answer to both is to print the error and exit. Here it is
 * logged, through the logger that redacts secrets, and the server carries
 * on: each request has its own error handling, and the state that matters
 * is in the database. Returns a function that takes the listeners off.
 */
export function guardProcess(log, proc = process) {
  const describe = (thrown) => (thrown instanceof Error ? thrown : new Error(String(thrown)));
  const onRejection = (reason) => log.error('unhandled rejection; the server carries on', describe(reason));
  const onException = (err, origin) => log.error(`${origin ?? 'uncaughtException'}; the server carries on`, describe(err));
  proc.on('unhandledRejection', onRejection);
  proc.on('uncaughtException', onException);
  return () => {
    proc.off('unhandledRejection', onRejection);
    proc.off('uncaughtException', onException);
  };
}

/**
 * Connects to the database (trying a few times: a name that does not
 * resolve at the first attempt often does at the second), reads the voice
 * list and starts listening. A database that still cannot be reached does
 * not stop the server: it listens, reports `database: false`, answers
 * `503 database_unavailable` where it needs the database, and connects by
 * itself when it can (`db.ensure`). Returns `{port, db, close}`; `close`
 * finishes requests in flight, then closes the pool.
 */
export async function start(config, { quiet = false, dbOptions, connectAttempts = 3, connectPauseMs = 2000 } = {}) {
  const log = createLogger({ secrets: secrets(config), quiet });
  if (!config.databaseUrl) throw new Error('DATABASE_URL is not set (see .env.example)');

  const db = createDb(config, log, dbOptions);
  for (let attempt = 1; attempt <= connectAttempts; attempt += 1) {
    try {
      await db.migrate();
      await db.warm();
      break;
    } catch (err) {
      log.error(`database not reachable at start (attempt ${attempt} of ${connectAttempts})`, err);
      if (attempt === connectAttempts) {
        log.warn('starting without the database: requests that need it get 503 until it answers');
      } else {
        await new Promise((resolve) => setTimeout(resolve, connectPauseMs));
      }
    }
  }
  const model = createModel(config, log);
  const voice = createVoice(config, log);
  await voice.init();
  if (!model.enabled) log.warn('DEEPSEEK_API_KEY is not set: calls cannot be played or scored');

  const app = createApp({ config, db, model, voice, log });
  const server = http.createServer((req, res) => {
    // `handle` answers its own errors; if it ever fails all the same, the
    // request gets a 500 and the process is not asked to judge it.
    app.handle(req, res).catch((err) => {
      log.error(`request handler failed on ${req.method}`, err);
      try {
        if (res.headersSent) {
          res.destroy();
        } else {
          res.writeHead(500, { 'Content-Type': 'application/json; charset=utf-8' });
          res.end('{"error":{"code":"internal_error","message":"Something went wrong on the server."}}');
        }
      } catch {
        res.destroy();
      }
    });
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
  guardProcess(fallback);
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
