// PostgreSQL: people, rooms and their chat, and what the server knows of
// calls, phone calls and recordings.
//
// The database is shared with other things (the Atlas and CallReady
// examples' tables among them), so this module only ever names tables
// prefixed `barua_talk_`. The migration is idempotent. Every query is
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
// functions (`test/fakes.js`), and `test/integration.test.js` runs the same
// routes against this one. Rows are plain objects with the column names
// below; times are `Date`s, message ids are numbers.

import pg from 'pg';

import { DatabaseDown } from './errors.js';

// Each entry runs once, in order, inside a transaction. Add to the end; do
// not edit an entry that has shipped. `@` stands for the table prefix
// (`barua_talk_`, with the schema in front when one is configured), `#`
// for the bare prefix (index names are never schema-qualified).
const MIGRATIONS = [
  `
  create table if not exists @people (
    device_id   text primary key,
    identity    text unique,
    name        text,
    created_at  timestamptz not null default now(),
    seen_at     timestamptz not null default now()
  );
  create table if not exists @rooms (
    name          text primary key,
    display_name  text not null,
    created_by    text not null,
    created_at    timestamptz not null default now(),
    active_at     timestamptz not null default now()
  );
  create table if not exists @members (
    room          text not null references @rooms (name) on delete cascade,
    identity      text not null,
    last_read_id  bigint not null default 0,
    joined_at     timestamptz not null default now(),
    primary key (room, identity)
  );
  create index if not exists #members_identity_idx on @members (identity);
  create table if not exists @messages (
    id             bigserial primary key,
    room           text not null references @rooms (name) on delete cascade,
    from_identity  text not null,
    text           text not null,
    client_id      text,
    delivered      boolean not null default false,
    created_at     timestamptz not null default now()
  );
  create index if not exists #messages_room_idx on @messages (room, id);
  create unique index if not exists #messages_client_idx
    on @messages (from_identity, client_id) where client_id is not null;
  create table if not exists @calls (
    id             text primary key,
    from_identity  text not null,
    to_identity    text not null,
    video          boolean not null default false,
    status         text not null,
    room           text,
    created_at     timestamptz not null default now(),
    answered_at    timestamptz,
    ended_at       timestamptz,
    seconds        integer not null default 0,
    checked_at     timestamptz not null default now()
  );
  create index if not exists #calls_to_idx on @calls (to_identity, created_at desc);
  create index if not exists #calls_from_idx on @calls (from_identity, created_at desc);
  create index if not exists #calls_live_idx on @calls (status) where status in ('ringing', 'active');
  create table if not exists @phone_calls (
    id           text primary key,
    upstream_id  text,
    identity     text not null,
    request_id   text,
    to_number    text not null,
    caller_id    text,
    status       text not null,
    created_at   timestamptz not null default now(),
    started_at   timestamptz,
    ended_at     timestamptz,
    seconds      integer not null default 0,
    price_tzs    integer,
    left_at      timestamptz
  );
  create index if not exists #phone_calls_identity_idx on @phone_calls (identity, created_at desc);
  create unique index if not exists #phone_calls_request_idx
    on @phone_calls (identity, request_id) where request_id is not null;
  create unique index if not exists #phone_calls_live_idx
    on @phone_calls (identity) where status in ('dialing', 'ringing', 'active');
  create table if not exists @recordings (
    id           text primary key,
    upstream_id  text,
    target_room  text not null,
    room         text,
    call_id      text,
    started_by   text not null,
    status       text not null,
    created_at   timestamptz not null default now(),
    ready_at     timestamptz,
    seconds      integer not null default 0,
    size_bytes   bigint,
    price_tzs    integer,
    url          text,
    url_at       timestamptz,
    checked_at   timestamptz not null default now()
  );
  create index if not exists #recordings_started_idx on @recordings (started_by, created_at desc);
  create unique index if not exists #recordings_running_idx
    on @recordings (target_room) where status in ('starting', 'recording');
  `,
  // Messages between two people. `low` and `high` are the two identities
  // in the order the server's own string comparison gives (never the
  // database's collation), so a pair has exactly one spelling.
  `
  create table if not exists @direct_messages (
    id             bigserial primary key,
    low            text not null,
    high           text not null,
    from_identity  text not null,
    text           text not null,
    client_id      text,
    created_at     timestamptz not null default now()
  );
  create index if not exists #direct_messages_pair_idx on @direct_messages (low, high, id);
  create index if not exists #direct_messages_high_idx on @direct_messages (high);
  create unique index if not exists #direct_messages_client_idx
    on @direct_messages (from_identity, client_id) where client_id is not null;
  create table if not exists @direct_reads (
    identity      text not null,
    peer          text not null,
    last_read_id  bigint not null default 0,
    primary key (identity, peer)
  );
  `,
];

/** The one spelling of a pair of identities: `[low, high]`. */
export function pairOf(a, b) {
  return a < b ? [a, b] : [b, a];
}

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
  return /connection terminated|timeout exceeded when trying to connect|connection timeout|query read timeout|client has encountered a connection error|getaddrinfo/i.test(
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

const UNIQUE_VIOLATION = '23505';

const CALL_COLUMNS = ['status', 'room', 'answered_at', 'ended_at', 'seconds', 'checked_at'];
const PHONE_COLUMNS = ['upstream_id', 'caller_id', 'status', 'started_at', 'ended_at', 'seconds', 'price_tzs', 'left_at'];
const RECORDING_COLUMNS = ['upstream_id', 'status', 'ready_at', 'seconds', 'size_bytes', 'price_tzs', 'url', 'url_at', 'checked_at'];

export function createDb(config, log, { Pool = pg.Pool, now = Date.now } = {}) {
  const pool = new Pool({
    ...connectionOptions(config.databaseUrl),
    max: 5,
    idleTimeoutMillis: 5 * 60 * 1000,
    connectionTimeoutMillis: 10000,
    statement_timeout: 15000,
    // The same limit on this side of the wire: a connection that has gone
    // silent (a network that dropped it without saying so) fails the query
    // instead of holding the request for ever.
    query_timeout: 20000,
    keepAlive: true,
    application_name: 'barua-talk-server',
  });
  // An idle client dropping its connection must not crash the process.
  pool.on('error', (err) => log.error('database idle connection error', err));

  // `databaseSchema` was checked against a strict pattern in `config.js`,
  // so it is safe to place in SQL text.
  const schema = config.databaseSchema;
  const T = schema ? `"${schema}".barua_talk_` : 'barua_talk_';
  /** Expands `@name` to the table and `#name` to an index name. */
  const sql = (text) => text.replace(/@(?=[a-z])/g, T).replace(/#(?=[a-z])/g, 'barua_talk_');

  let migrated = false;
  let migrating = null;
  let lastFailure = -Infinity;

  async function migrate() {
    const client = await pool.connect();
    try {
      await client.query('begin');
      // Two servers starting together must not race each other.
      await client.query('select pg_advisory_xact_lock(hashtext($1))', [`barua_talk_schema_version:${schema}`]);
      if (schema) await client.query(`create schema if not exists "${schema}"`);
      await client.query(
        sql(`create table if not exists @schema_version (
               version    integer primary key,
               applied_at timestamptz not null default now()
             )`),
      );
      const { rows } = await client.query(sql('select coalesce(max(version), 0)::int as version from @schema_version'));
      for (let version = rows[0].version + 1; version <= MIGRATIONS.length; version += 1) {
        await client.query(sql(MIGRATIONS[version - 1]));
        await client.query(sql('insert into @schema_version (version) values ($1)'), [version]);
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
      return await pool.query(sql(text), params);
    } catch (err) {
      if (isConnectionError(err)) {
        log.error('database query could not connect', err);
        throw new DatabaseDown(err);
      }
      throw err;
    }
  }

  async function one(text, params) {
    const { rows } = await query(text, params);
    return rows[0] ?? null;
  }

  /** `update … set` for the allowed columns present in `patch`. */
  async function patchRow(table, allowed, id, patch, from) {
    const names = allowed.filter((name) => patch[name] !== undefined);
    if (names.length === 0) return one(`select * from @${table} where id = $1`, [id]);
    const sets = names.map((name, i) => `${name} = $${i + 2}`);
    const params = [id, ...names.map((name) => patch[name])];
    let where = 'id = $1';
    if (from) {
      params.push(from);
      where += ` and status = any($${params.length})`;
    }
    return one(`update @${table} set ${sets.join(', ')} where ${where} returning *`, params);
  }

  const message = (row) => (row ? { ...row, id: Number(row.id) } : null);
  const recording = (row) => (row ? { ...row, size_bytes: row.size_bytes === null ? null : Number(row.size_bytes) } : null);

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

    /** Removes a throwaway schema made by the tests, and nothing else. */
    async dropTestSchema() {
      if (!schema.startsWith('barua_talk_test_')) throw new Error('only a barua_talk_test_ schema may be dropped');
      await pool.query(`drop schema if exists "${schema}" cascade`);
      migrated = false;
    },

    /** Empties every table of a throwaway schema made by the tests. */
    async truncateTestData() {
      if (!schema.startsWith('barua_talk_test_')) throw new Error('only a barua_talk_test_ schema may be emptied');
      await query(
        `truncate @people, @rooms, @members, @messages, @calls, @phone_calls, @recordings,
                  @direct_messages, @direct_reads restart identity cascade`,
      );
    },

    // ---- people ---------------------------------------------------------

    getPerson(deviceId) {
      return one('select * from @people where device_id = $1', [deviceId]);
    },

    /**
     * Creates the person or updates them: an identity is only ever given
     * to someone who has none, a name replaces the old one when given.
     * Returns the row, or null when `identity` belongs to someone else.
     */
    async upsertPerson(deviceId, { identity = null, name = null, at }) {
      try {
        return await one(
          `insert into @people (device_id, identity, name, created_at, seen_at)
           values ($1, $2, $3, $4, $4)
           on conflict (device_id) do update set
             identity = coalesce(@people.identity, excluded.identity),
             name     = coalesce(excluded.name, @people.name),
             seen_at  = excluded.seen_at
           returning *`,
          [deviceId, identity, name, at],
        );
      } catch (err) {
        if (err.code === UNIQUE_VIOLATION) return null;
        throw err;
      }
    },

    async touchPerson(deviceId, at) {
      await query('update @people set seen_at = $2 where device_id = $1', [deviceId, at]);
    },

    /** Everyone with an identity except `identity`, last seen first. */
    async listPeople(identity, limit) {
      const { rows } = await query(
        `select * from @people where identity is not null and identity <> $1
          order by seen_at desc, identity limit $2`,
        [identity, limit],
      );
      return rows;
    },

    async peopleByIdentity(identities) {
      if (identities.length === 0) return [];
      const { rows } = await query('select * from @people where identity = any($1)', [identities]);
      return rows;
    },

    // ---- rooms ----------------------------------------------------------

    /** Returns `{row, created}`; an existing room is left as it is. */
    async createRoom({ name, displayName, createdBy, at }) {
      const made = await one(
        `insert into @rooms (name, display_name, created_by, created_at, active_at)
         values ($1, $2, $3, $4, $4) on conflict (name) do nothing returning *`,
        [name, displayName, createdBy, at],
      );
      if (made) return { row: made, created: true };
      return { row: await one('select * from @rooms where name = $1', [name]), created: false };
    },

    getRoom(name) {
      return one('select * from @rooms where name = $1', [name]);
    },

    async deleteRoom(name) {
      const { rowCount } = await query('delete from @rooms where name = $1', [name]);
      return rowCount > 0;
    },

    async touchRoom(name, at) {
      await query('update @rooms set active_at = greatest(active_at, $2) where name = $1', [name, at]);
    },

    /**
     * Rooms as `identity` sees them, most recently active first: the room's
     * columns plus `members`, `member`, `unread` and the last message as
     * `last_id`, `last_from`, `last_text`, `last_at`, `last_delivered`
     * (null without one). `name`: that room only. `memberOnly`: only the
     * rooms `identity` is a member of.
     */
    async listRooms(identity, { name = null, memberOnly = false, limit = 100 } = {}) {
      const { rows } = await query(
        `select r.name, r.display_name, r.created_by, r.created_at, r.active_at,
                (select count(*) from @members m where m.room = r.name)::int as members,
                (me.identity is not null) as member,
                coalesce((select count(*) from @messages x
                           where x.room = r.name and x.id > me.last_read_id and x.from_identity <> $1), 0)::int as unread,
                lm.id as last_id, lm.from_identity as last_from, lm.text as last_text,
                lm.created_at as last_at, lm.delivered as last_delivered
           from @rooms r
           left join @members me on me.room = r.name and me.identity = $1
           left join lateral (
             select x.id, x.from_identity, x.text, x.created_at, x.delivered
               from @messages x where x.room = r.name order by x.id desc limit 1
           ) lm on true
          where ($2::text is null or r.name = $2)
            and (not $3::boolean or me.identity is not null)
          order by r.active_at desc, r.name
          limit $4`,
        [identity, name, memberOnly, limit],
      );
      return rows.map((row) => ({ ...row, last_id: row.last_id === null ? null : Number(row.last_id) }));
    },

    /** Makes `identity` a member (their read mark starts at 0). */
    async joinRoom(room, identity, at) {
      await query(
        `insert into @members (room, identity, joined_at) values ($1, $2, $3)
         on conflict (room, identity) do nothing`,
        [room, identity, at],
      );
    },

    /**
     * Moves the read mark forward to `upTo` (null: the newest message),
     * never beyond the newest message and never backwards. Makes the
     * person a member. Returns their unread count afterwards.
     */
    async markRead(room, identity, upTo, at) {
      const row = await one(
        `with mark as (
           insert into @members (room, identity, last_read_id, joined_at)
           select $1::text, $2::text, least(coalesce($3::bigint, top.id), top.id), $4::timestamptz
             from (select coalesce(max(id), 0) as id from @messages where room = $1::text) top
           on conflict (room, identity) do update set
             last_read_id = greatest(@members.last_read_id, excluded.last_read_id)
           returning last_read_id
         )
         select count(*)::int as unread
           from @messages x
          where x.room = $1::text and x.from_identity <> $2::text and x.id > (select last_read_id from mark)`,
        [room, identity, upTo, at],
      );
      return row.unread;
    },

    async unread(room, identity) {
      const row = await one(
        `select count(*)::int as unread
           from @messages x join @members me on me.room = x.room and me.identity = $2
          where x.room = $1 and x.id > me.last_read_id and x.from_identity <> $2`,
        [room, identity],
      );
      return row.unread;
    },

    // ---- chat -----------------------------------------------------------

    /**
     * Stores a message and marks the room active. With a `clientId` the
     * same person has already used, nothing is stored and that first
     * message is returned: `{row, created}`.
     */
    async addMessage({ room, from, text, clientId = null, at }) {
      const made = await one(
        `with line as (
           insert into @messages (room, from_identity, text, client_id, created_at)
           values ($1, $2, $3, $4, $5)
           on conflict (from_identity, client_id) where client_id is not null do nothing
           returning *
         ), touched as (
           update @rooms set active_at = greatest(active_at, $5) where name = $1 and exists (select 1 from line)
         )
         select * from line`,
        [room, from, text, clientId, at],
      );
      if (made) return { row: message(made), created: true };
      const first = await one('select * from @messages where from_identity = $1 and client_id = $2', [from, clientId]);
      return { row: message(first), created: false };
    },

    async setDelivered(id, delivered) {
      await query('update @messages set delivered = $2 where id = $1', [id, delivered]);
    },

    /**
     * `after` null: the latest `limit` messages. Otherwise the first
     * `limit` with a larger id. Oldest first either way. Returns
     * `{rows, lastId, more}`.
     */
    async listMessages(room, { after = null, limit }) {
      const page =
        after === null
          ? query('select * from @messages where room = $1 order by id desc limit $2', [room, limit])
          : query('select * from @messages where room = $1 and id > $2 order by id limit $3', [room, after, limit]);
      const [top, found] = await Promise.all([
        one('select coalesce(max(id), 0) as id from @messages where room = $1', [room]),
        page,
      ]);
      const lastId = Number(top.id);
      let { rows } = found;
      if (after === null) rows.reverse();
      rows = rows.map(message);
      const more = after !== null && rows.length > 0 && rows[rows.length - 1].id < lastId;
      return { rows, lastId, more };
    },

    // ---- messages between two people ------------------------------------

    /**
     * Stores a message from one person to another. With a `clientId` the
     * sender has already used, nothing is stored and that first message is
     * returned: `{row, created}`.
     */
    async addDirect({ from, to, text, clientId = null, at }) {
      const [low, high] = pairOf(from, to);
      const made = await one(
        `insert into @direct_messages (low, high, from_identity, text, client_id, created_at)
         values ($1, $2, $3, $4, $5, $6)
         on conflict (from_identity, client_id) where client_id is not null do nothing
         returning *`,
        [low, high, from, text, clientId, at],
      );
      if (made) return { row: message(made), created: true };
      const first = await one('select * from @direct_messages where from_identity = $1 and client_id = $2', [from, clientId]);
      return { row: message(first), created: false };
    },

    /** As `listMessages`, for the conversation between `a` and `b`. */
    async listDirect(a, b, { after = null, limit }) {
      const [low, high] = pairOf(a, b);
      const page =
        after === null
          ? query('select * from @direct_messages where low = $1 and high = $2 order by id desc limit $3', [low, high, limit])
          : query('select * from @direct_messages where low = $1 and high = $2 and id > $3 order by id limit $4', [low, high, after, limit]);
      const [top, found] = await Promise.all([
        one('select coalesce(max(id), 0) as id from @direct_messages where low = $1 and high = $2', [low, high]),
        page,
      ]);
      const lastId = Number(top.id);
      let { rows } = found;
      if (after === null) rows.reverse();
      rows = rows.map(message);
      const more = after !== null && rows.length > 0 && rows[rows.length - 1].id < lastId;
      return { rows, lastId, more };
    },

    /**
     * Moves `identity`'s read mark in the conversation with `peer` forward
     * to `upTo` (null: the newest message), never beyond the newest and
     * never backwards. Returns their unread count afterwards.
     */
    async markDirectRead(identity, peer, upTo) {
      const [low, high] = pairOf(identity, peer);
      const row = await one(
        `with mark as (
           insert into @direct_reads (identity, peer, last_read_id)
           select $1::text, $2::text, least(coalesce($5::bigint, top.id), top.id)
             from (select coalesce(max(id), 0) as id from @direct_messages where low = $3::text and high = $4::text) top
           on conflict (identity, peer) do update set
             last_read_id = greatest(@direct_reads.last_read_id, excluded.last_read_id)
           returning last_read_id
         )
         select count(*)::int as unread
           from @direct_messages x
          where x.low = $3::text and x.high = $4::text and x.from_identity <> $1::text
            and x.id > (select last_read_id from mark)`,
        [identity, peer, low, high, upTo],
      );
      return row.unread;
    },

    /**
     * The conversations `identity` has, newest message first: `peer`,
     * `unread` and the last message as `last_id`, `last_from`,
     * `last_text`, `last_at`.
     */
    async listConversations(identity, limit) {
      const { rows } = await query(
        `select c.peer, lm.id as last_id, lm.from_identity as last_from, lm.text as last_text, lm.created_at as last_at,
                (select count(*) from @direct_messages x
                  where ((x.low = $1 and x.high = c.peer) or (x.low = c.peer and x.high = $1))
                    and x.from_identity <> $1 and x.id > coalesce(r.last_read_id, 0))::int as unread
           from (
             select high as peer from @direct_messages where low = $1
             union
             select low as peer from @direct_messages where high = $1
           ) c
           left join @direct_reads r on r.identity = $1 and r.peer = c.peer
           join lateral (
             select x.id, x.from_identity, x.text, x.created_at
               from @direct_messages x
              where (x.low = $1 and x.high = c.peer) or (x.low = c.peer and x.high = $1)
              order by x.id desc limit 1
           ) lm on true
          order by lm.id desc
          limit $2`,
        [identity, limit],
      );
      return rows.map((row) => ({ ...row, last_id: Number(row.last_id) }));
    },

    // ---- in-app calls ---------------------------------------------------

    insertCall({ id, from, to, video, status, room = null, at }) {
      return one(
        `insert into @calls (id, from_identity, to_identity, video, status, room, created_at, checked_at)
         values ($1, $2, $3, $4, $5, $6, $7, $7)
         on conflict (id) do update set id = excluded.id
         returning *`,
        [id, from, to, video, status, room, at],
      );
    },

    getCall(id) {
      return one('select * from @calls where id = $1', [id]);
    },

    /**
     * Applies `patch`. With `from` (a list of statuses) only if the call
     * is in one of them; returns null if it was not.
     */
    updateCall(id, patch, from) {
      return patchRow('calls', CALL_COLUMNS, id, patch, from);
    },

    /** The calls ringing for `identity`, oldest first. */
    async ringingFor(identity) {
      const { rows } = await query(
        `select * from @calls where to_identity = $1 and status = 'ringing' order by created_at, id`,
        [identity],
      );
      return rows;
    },

    async listCalls(identity, limit) {
      const { rows } = await query(
        `select * from @calls where from_identity = $1 or to_identity = $1
          order by created_at desc, id limit $2`,
        [identity, limit],
      );
      return rows;
    },

    /** Every call that is ringing or active, whoever's. */
    async liveCalls() {
      const { rows } = await query(`select * from @calls where status in ('ringing', 'active') order by created_at`);
      return rows;
    },

    // ---- phone calls ----------------------------------------------------

    /**
     * Sets a phone call down before it is dialled, so that two requests
     * cannot both dial. Returns one of:
     * `{row, created: true}`: go ahead and dial;
     * `{row, created: false}`: this `requestId` was used before, here is its call;
     * `{busy: true}`: the person has a phone call that has not ended.
     */
    async reservePhoneCall({ id, identity, requestId = null, to, at }) {
      const byRequest = () =>
        requestId === null
          ? null
          : one('select * from @phone_calls where identity = $1 and request_id = $2', [identity, requestId]);
      const earlier = await byRequest();
      if (earlier) return { row: earlier, created: false };
      try {
        const row = await one(
          `insert into @phone_calls (id, identity, request_id, to_number, status, created_at)
           values ($1, $2, $3, $4, 'dialing', $5) returning *`,
          [id, identity, requestId, to, at],
        );
        return { row, created: true };
      } catch (err) {
        if (err.code !== UNIQUE_VIOLATION) throw err;
        const raced = await byRequest();
        return raced ? { row: raced, created: false } : { busy: true };
      }
    },

    getPhoneCall(id) {
      return one('select * from @phone_calls where id = $1', [id]);
    },

    updatePhoneCall(id, patch, from) {
      return patchRow('phone_calls', PHONE_COLUMNS, id, patch, from);
    },

    async deletePhoneCall(id) {
      await query('delete from @phone_calls where id = $1', [id]);
    },

    async listPhoneCalls(identity, limit) {
      const { rows } = await query(
        'select * from @phone_calls where identity = $1 order by created_at desc, id limit $2',
        [identity, limit],
      );
      return rows;
    },

    /** Every phone call upstream may still be running. */
    async livePhoneCalls() {
      const { rows } = await query(
        `select * from @phone_calls where status in ('ringing', 'active') and upstream_id is not null`,
      );
      return rows;
    },

    /** The caller id of the latest call that had one, or null. */
    async lastCallerId() {
      const row = await one(
        'select caller_id from @phone_calls where caller_id is not null order by created_at desc limit 1',
      );
      return row ? row.caller_id : null;
    },

    // ---- recordings -----------------------------------------------------

    /**
     * Sets a recording down before it is started. `{row, created: true}`:
     * go ahead; `{row, created: false}`: `targetRoom` is being recorded
     * already, by `row`.
     */
    async reserveRecording({ id, targetRoom, room = null, callId = null, startedBy, at }) {
      const running = () =>
        one(`select * from @recordings where target_room = $1 and status in ('starting', 'recording')`, [targetRoom]);
      const earlier = await running();
      if (earlier) return { row: recording(earlier), created: false };
      try {
        const row = await one(
          `insert into @recordings (id, target_room, room, call_id, started_by, status, created_at, checked_at)
           values ($1, $2, $3, $4, $5, 'starting', $6, $6) returning *`,
          [id, targetRoom, room, callId, startedBy, at],
        );
        return { row: recording(row), created: true };
      } catch (err) {
        if (err.code !== UNIQUE_VIOLATION) throw err;
        const raced = await running();
        if (!raced) throw err;
        return { row: recording(raced), created: false };
      }
    },

    async getRecording(id) {
      return recording(await one('select * from @recordings where id = $1', [id]));
    },

    async updateRecording(id, patch, from) {
      return recording(await patchRow('recordings', RECORDING_COLUMNS, id, patch, from));
    },

    async deleteRecording(id) {
      await query('delete from @recordings where id = $1', [id]);
    },

    /** By one of `startedBy`, `room`, `callId`; newest first. */
    async listRecordings({ startedBy = null, room = null, callId = null }, limit) {
      const { rows } = await query(
        `select * from @recordings
          where ($1::text is not null and started_by = $1)
             or ($2::text is not null and room = $2)
             or ($3::text is not null and call_id = $3)
          order by created_at desc, id limit $4`,
        [startedBy, room, callId, limit],
      );
      return rows.map(recording);
    },

    /** Every recording that is running, whoever's. */
    async runningRecordings() {
      const { rows } = await query(`select * from @recordings where status = 'recording' order by created_at`);
      return rows.map(recording);
    },

    /**
     * Gives up on what was set down before `before` and never got its
     * answer from upstream (a server that stopped mid-request): such a row
     * would otherwise block its person or its room for ever.
     */
    async failStale(before) {
      await query(`update @phone_calls set status = 'failed', ended_at = now() where status = 'dialing' and created_at < $1`, [before]);
      await query(`update @recordings set status = 'failed' where status = 'starting' and created_at < $1`, [before]);
    },

    /** The charges upstream has reported, in shillings. */
    async spentTzs() {
      const row = await one(
        `select (coalesce((select sum(price_tzs) from @phone_calls), 0)
               + coalesce((select sum(price_tzs) from @recordings), 0))::int as spent`,
      );
      return row.spent;
    },
  };
}
