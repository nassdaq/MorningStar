// PostgreSQL: profiles, chats and their messages.
//
// The database is shared with other things (the CallReady example's
// `callready_*` tables among them), so this module only ever names tables
// prefixed `atlas_`. The migration is idempotent. Every query is
// parameterised.
//
// The database may be unreachable when the server starts, or stop
// answering later (a name that does not resolve for a minute, a restart on
// the other side). Neither stops the server: every operation first makes
// sure the schema is there (`ensure`), a failure to connect is reported as
// `DatabaseDown` (a 503 to the app, `database: false` in the health
// check), and the next request tries again, at most once every
// `databaseRetryMs`. Nothing has to be restarted.
//
// The functions returned by `createDb` are the whole storage interface; the
// unit tests run the routes against an in-memory implementation of the same
// functions (`test/fakes.js`).

import pg from 'pg';

import { DatabaseDown } from './errors.js';

// Each entry runs once, in order, inside a transaction. Add to the end; do
// not edit an entry that has shipped.
const MIGRATIONS = [
  `
  create table if not exists atlas_profiles (
    device_id   text primary key,
    model       text not null default 'auto',
    plan        text not null default 'free',
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
  );
  create table if not exists atlas_chats (
    id          text primary key,
    device_id   text not null references atlas_profiles (device_id) on delete cascade,
    title       text not null,
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
  );
  create index if not exists atlas_chats_device_idx
    on atlas_chats (device_id, updated_at desc);
  create table if not exists atlas_messages (
    chat_id     text not null references atlas_chats (id) on delete cascade,
    seq         integer not null,
    role        text not null check (role in ('user', 'assistant')),
    text        text not null,
    attachments jsonb,
    steps       jsonb,
    model_used  text,
    created_at  timestamptz not null default now(),
    primary key (chat_id, seq)
  );
  `,
];

const NETWORK_CODES = new Set([
  'ENOTFOUND', 'EAI_AGAIN', 'ECONNREFUSED', 'ECONNRESET', 'ETIMEDOUT', 'EPIPE', 'EHOSTUNREACH', 'ENETUNREACH',
]);

/**
 * Whether an error from `pg` means "the database could not be reached" (as
 * opposed to "the query was wrong", which is a bug and stays a 500).
 */
export function isConnectionError(err) {
  if (!err) return false;
  const code = String(err.code ?? '');
  if (NETWORK_CODES.has(code)) return true;
  // SQLSTATE class 08 (connection exception), 57P0x (shutdown), 53300 (too many connections).
  if (/^08/.test(code) || /^57P0/.test(code) || code === '53300') return true;
  return /connection terminated|timeout exceeded when trying to connect|connection timeout|client has encountered a connection error|getaddrinfo/i.test(
    String(err.message ?? ''),
  );
}

/**
 * Pool options for a connection URL. `sslmode=require` (and `prefer`,
 * `verify-ca`, `verify-full`) all mean: encrypt and verify the server's
 * certificate. That is what `pg` does with them today; saying so explicitly
 * keeps it that way when its defaults change.
 */
export function connectionOptions(databaseUrl) {
  try {
    const url = new URL(databaseUrl);
    const mode = url.searchParams.get('sslmode');
    if (['prefer', 'require', 'verify-ca', 'verify-full'].includes(mode)) {
      url.searchParams.delete('sslmode');
      return { connectionString: url.toString(), ssl: { rejectUnauthorized: true } };
    }
  } catch {
    // Not a URL (a key=value string, say): let pg read it as it is.
  }
  return { connectionString: databaseUrl };
}

export function createDb(config, log, { Pool = pg.Pool, now = Date.now } = {}) {
  const pool = new Pool({
    ...connectionOptions(config.databaseUrl),
    max: 5,
    idleTimeoutMillis: 5 * 60 * 1000,
    connectionTimeoutMillis: 10000,
    statement_timeout: 15000,
    application_name: 'atlas-server',
  });
  // An idle client dropping its connection must not crash the process.
  pool.on('error', (err) => log.error('database idle connection error', err));

  let migrated = false;
  let migrating = null;
  let lastFailure = -Infinity;

  async function migrate() {
    const client = await pool.connect();
    try {
      await client.query('begin');
      // Two servers starting together must not race each other.
      await client.query("select pg_advisory_xact_lock(hashtext('atlas_schema_version'))");
      await client.query(
        `create table if not exists atlas_schema_version (
           version    integer primary key,
           applied_at timestamptz not null default now()
         )`,
      );
      const { rows } = await client.query('select coalesce(max(version), 0)::int as version from atlas_schema_version');
      for (let version = rows[0].version + 1; version <= MIGRATIONS.length; version += 1) {
        await client.query(MIGRATIONS[version - 1]);
        await client.query('insert into atlas_schema_version (version) values ($1)', [version]);
        log.info(`database migrated to schema version ${version}`);
      }
      await client.query('commit');
    } catch (err) {
      await client.query('rollback').catch(() => {});
      throw err;
    } finally {
      client.release();
    }
  }

  /**
   * Makes sure the schema is there. One attempt at a time, and after a
   * failed one no other for `databaseRetryMs`: a database that is down is
   * not hammered by every request, and one that comes back is found by the
   * first request after that pause.
   */
  function ensure() {
    if (migrated) return Promise.resolve();
    if (migrating) return migrating;
    if (now() - lastFailure < config.databaseRetryMs) return Promise.reject(new DatabaseDown());
    migrating = (async () => {
      try {
        await migrate();
        migrated = true;
        log.info('database ready');
      } catch (err) {
        lastFailure = now();
        log.error('database not reachable; will try again', err);
        throw new DatabaseDown(err);
      } finally {
        migrating = null;
      }
    })();
    return migrating;
  }

  async function query(text, params) {
    await ensure();
    try {
      return await pool.query(text, params);
    } catch (err) {
      if (isConnectionError(err)) {
        log.error('database query could not connect', err);
        throw new DatabaseDown(err);
      }
      throw err;
    }
  }

  return {
    /** Runs the migration now, whatever happened before. Throws on failure. */
    async migrate() {
      await migrate();
      migrated = true;
    },

    ensure,

    /** Whether the schema is known to be in place. */
    ready: () => migrated,

    async ping() {
      await query('select 1');
    },

    async close() {
      await pool.end();
    },

    /** The device's profile row, created with defaults on first sight. */
    async getOrCreateProfile(deviceId) {
      const { rows } = await query(
        `insert into atlas_profiles (device_id) values ($1)
         on conflict (device_id) do update set device_id = excluded.device_id
         returning device_id, model, plan`,
        [deviceId],
      );
      return rows[0];
    },

    /** Applies `patch` (any of model, plan). */
    async updateProfile(deviceId, patch) {
      const { rows } = await query(
        `insert into atlas_profiles (device_id, model, plan)
         values ($1, coalesce($2, 'auto'), coalesce($3, 'free'))
         on conflict (device_id) do update set
           model      = coalesce($2, atlas_profiles.model),
           plan       = coalesce($3, atlas_profiles.plan),
           updated_at = now()
         returning device_id, model, plan`,
        [deviceId, patch.model ?? null, patch.plan ?? null],
      );
      return rows[0];
    },

    /** The device's chats, most recently updated first. */
    async listChats(deviceId, limit) {
      const { rows } = await query(
        `select c.id, c.title, c.updated_at,
                (select count(*) from atlas_messages m where m.chat_id = c.id and m.role = 'user')::int as messages
           from atlas_chats c
          where c.device_id = $1
          order by c.updated_at desc, c.id
          limit $2`,
        [deviceId, limit],
      );
      return rows;
    },

    async getChat(id) {
      const { rows } = await query('select id, device_id, title, created_at, updated_at from atlas_chats where id = $1', [id]);
      return rows[0] ?? null;
    },

    /** The last `limit` messages of a chat, oldest first. */
    async getMessages(chatId, limit) {
      const { rows } = await query(
        `select seq, role, text, attachments, steps, model_used
           from atlas_messages where chat_id = $1
          order by seq desc
          limit $2`,
        [chatId, limit],
      );
      return rows.reverse();
    },

    /**
     * Stores a new chat with its first exchange. One statement: the chat
     * and both messages exist together or not at all.
     */
    async createChat({ id, deviceId, title, user, assistant, at }) {
      await query(
        `with owner as (
           insert into atlas_profiles (device_id) values ($2::text)
           on conflict (device_id) do update set device_id = excluded.device_id
           returning device_id
         ), chat as (
           insert into atlas_chats (id, device_id, title, created_at, updated_at)
           select $1::text, device_id, $3::text, $4::timestamptz, $4::timestamptz from owner
           returning id
         )
         insert into atlas_messages (chat_id, seq, role, text, attachments, steps, model_used, created_at)
         select chat.id, v.seq, v.role, v.text, v.attachments, v.steps, v.model_used, $4::timestamptz
           from chat, (values
             (1, 'user', $5::text, $6::jsonb, null::jsonb, null::text),
             (2, 'assistant', $7::text, null::jsonb, $8::jsonb, $9::text)
           ) as v (seq, role, text, attachments, steps, model_used)`,
        [id, deviceId, title, at, user.text, JSON.stringify(user.attachments ?? []), assistant.text, JSON.stringify(assistant.steps), assistant.modelUsed],
      );
    },

    /**
     * Appends a message and its answer after message `seq` (the last one
     * stored). One statement: both or neither.
     */
    async addExchange(chatId, { seq, user, assistant, at }) {
      await query(
        `with lines as (
           insert into atlas_messages (chat_id, seq, role, text, attachments, steps, model_used, created_at) values
             ($1::text, $2::int + 1, 'user', $4::text, $5::jsonb, null, null, $3::timestamptz),
             ($1::text, $2::int + 2, 'assistant', $6::text, null, $7::jsonb, $8::text, $3::timestamptz)
         )
         update atlas_chats set updated_at = $3::timestamptz where id = $1::text`,
        [chatId, seq, at, user.text, JSON.stringify(user.attachments ?? []), assistant.text, JSON.stringify(assistant.steps), assistant.modelUsed],
      );
    },

    /** Deletes a chat of this device. Returns whether there was one. */
    async deleteChat(id, deviceId) {
      const { rowCount } = await query('delete from atlas_chats where id = $1 and device_id = $2', [id, deviceId]);
      return rowCount > 0;
    },

    /**
     * Removes everything stored for test devices (ids starting `test_`); the
     * chats and messages go with the profile. Used by the test suite. With
     * `deviceId` only that device is removed, and it must be a test id.
     */
    async deleteTestData(deviceId) {
      if (deviceId !== undefined) {
        if (!deviceId.startsWith('test_')) throw new Error('only test_ devices may be deleted');
        const { rowCount } = await query('delete from atlas_profiles where device_id = $1', [deviceId]);
        return rowCount;
      }
      const { rowCount } = await query(`delete from atlas_profiles where left(device_id, 5) = 'test_'`);
      return rowCount;
    },
  };
}
