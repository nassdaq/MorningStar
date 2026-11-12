// PostgreSQL: profiles, calls, turns and reports.
//
// The database is shared with other things, so this module only ever names
// tables prefixed `callready_`. The migration is idempotent and runs at
// start-up. Every query is parameterised.
//
// The functions returned by `createDb` are the whole storage interface; the
// unit tests run the routes against an in-memory implementation of the same
// functions (`test/fakes.js`).

import pg from 'pg';

// Each entry runs once, in order, inside a transaction. Add to the end; do
// not edit an entry that has shipped.
const MIGRATIONS = [
  `
  create table if not exists callready_profiles (
    device_id   text primary key,
    name        text not null default '',
    accent      text not null default 'us',
    difficulty  text not null default 'standard',
    minutes     integer not null default 4,
    plan        text not null default 'free',
    created_at  timestamptz not null default now(),
    updated_at  timestamptz not null default now()
  );
  create table if not exists callready_calls (
    id               text primary key,
    device_id        text not null references callready_profiles (device_id) on delete cascade,
    number           integer not null,
    scenario         text not null,
    accent           text not null,
    difficulty       text not null,
    minutes          integer not null,
    voice            boolean not null default false,
    status           text not null default 'active' check (status in ('active', 'ended')),
    caller_done      boolean not null default false,
    started_at       timestamptz not null default now(),
    ended_at         timestamptz,
    duration_seconds integer
  );
  create index if not exists callready_calls_device_idx
    on callready_calls (device_id, started_at desc);
  create table if not exists callready_turns (
    call_id    text not null references callready_calls (id) on delete cascade,
    seq        integer not null,
    speaker    text not null check (speaker in ('caller', 'learner')),
    text       text not null,
    tips       jsonb,
    note       text,
    created_at timestamptz not null default now(),
    primary key (call_id, seq)
  );
  create table if not exists callready_reports (
    call_id               text primary key references callready_calls (id) on delete cascade,
    score                 integer not null,
    clarity               integer not null,
    empathy               integer not null,
    listening             integer not null,
    problem_solving       integer not null,
    professional_language integer not null,
    did_well              text not null,
    try_next              text not null,
    created_at            timestamptz not null default now()
  );
  `,
];

const CALL_COLUMNS =
  'id, device_id, number, scenario, accent, difficulty, minutes, voice, status, caller_done, started_at, ended_at, duration_seconds';

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

export function createDb(config, log) {
  const pool = new pg.Pool({
    ...connectionOptions(config.databaseUrl),
    max: 5,
    // The database is remote and a new TLS connection costs most of a second,
    // so two are kept open between requests.
    min: 2,
    idleTimeoutMillis: 5 * 60 * 1000,
    connectionTimeoutMillis: 10000,
    statement_timeout: 15000,
    application_name: 'callready-server',
  });
  // An idle client dropping its connection must not crash the process.
  pool.on('error', (err) => log.error('database idle connection error', err));

  async function transaction(work) {
    const client = await pool.connect();
    try {
      await client.query('begin');
      const result = await work(client);
      await client.query('commit');
      return result;
    } catch (err) {
      await client.query('rollback').catch(() => {});
      throw err;
    } finally {
      client.release();
    }
  }

  return {
    async migrate() {
      await transaction(async (client) => {
        // Two servers starting together must not race each other.
        await client.query("select pg_advisory_xact_lock(hashtext('callready_schema_version'))");
        await client.query(
          `create table if not exists callready_schema_version (
             version    integer primary key,
             applied_at timestamptz not null default now()
           )`,
        );
        const { rows } = await client.query('select coalesce(max(version), 0)::int as version from callready_schema_version');
        for (let version = rows[0].version + 1; version <= MIGRATIONS.length; version += 1) {
          await client.query(MIGRATIONS[version - 1]);
          await client.query('insert into callready_schema_version (version) values ($1)', [version]);
          log.info(`database migrated to schema version ${version}`);
        }
      });
    },

    async ping() {
      await pool.query('select 1');
    },

    /** Opens the connections the first requests will need. */
    async warm() {
      await Promise.all([pool.query('select 1'), pool.query('select 1')]);
    },

    async close() {
      await pool.end();
    },

    /** The device's profile row, created with defaults on first sight. */
    async getOrCreateProfile(deviceId) {
      const { rows } = await pool.query(
        `insert into callready_profiles (device_id) values ($1)
         on conflict (device_id) do update set device_id = excluded.device_id
         returning device_id, name, accent, difficulty, minutes, plan`,
        [deviceId],
      );
      return rows[0];
    },

    /** Applies `patch` (any of name, accent, difficulty, minutes, plan). */
    async updateProfile(deviceId, patch) {
      const { rows } = await pool.query(
        `insert into callready_profiles (device_id, name, accent, difficulty, minutes, plan)
         values ($1, coalesce($2, ''), coalesce($3, 'us'), coalesce($4, 'standard'), coalesce($5, 4), coalesce($6, 'free'))
         on conflict (device_id) do update set
           name       = coalesce($2, callready_profiles.name),
           accent     = coalesce($3, callready_profiles.accent),
           difficulty = coalesce($4, callready_profiles.difficulty),
           minutes    = coalesce($5, callready_profiles.minutes),
           plan       = coalesce($6, callready_profiles.plan),
           updated_at = now()
         returning device_id, name, accent, difficulty, minutes, plan`,
        [deviceId, patch.name ?? null, patch.accent ?? null, patch.difficulty ?? null, patch.minutes ?? null, patch.plan ?? null],
      );
      return rows[0];
    },

    /**
     * What the profile shows about past calls: how many were scored, their
     * mean score, and the distinct days (in `timeZone`, newest first) on
     * which one was started.
     */
    async stats(deviceId, timeZone) {
      const [totals, days] = await Promise.all([
        pool.query(
          `select count(*)::int as calls, round(avg(r.score))::int as average_score
             from callready_calls c join callready_reports r on r.call_id = c.id
            where c.device_id = $1`,
          [deviceId],
        ),
        pool.query(
          `select distinct to_char(c.started_at at time zone $2, 'YYYY-MM-DD') as day
             from callready_calls c join callready_reports r on r.call_id = c.id
            where c.device_id = $1
            order by day desc
            limit 400`,
          [deviceId, timeZone],
        ),
      ]);
      return {
        calls: totals.rows[0].calls,
        average_score: totals.rows[0].average_score,
        days: days.rows.map((row) => row.day),
      };
    },

    /**
     * Starts a call and stores the caller's opening line as turn 1. The
     * call's number is one more than the device's highest scored call.
     */
    async createCall({ id, deviceId, scenario, accent, difficulty, minutes, voice, opening, tips, startedAt }) {
      // One statement, so the call and its first turn exist together or not at all.
      const { rows } = await pool.query(
        `with new_call as (
           insert into callready_calls (id, device_id, number, scenario, accent, difficulty, minutes, voice, started_at)
           select $1::text, $2::text,
                  (select coalesce(max(c.number), 0) + 1
                     from callready_calls c join callready_reports r on r.call_id = c.id
                    where c.device_id = $2::text),
                  $3::text, $4::text, $5::text, $6::int, $7::boolean, $8::timestamptz
           returning ${CALL_COLUMNS}
         ), opening as (
           insert into callready_turns (call_id, seq, speaker, text, tips)
           select id, 1, 'caller', $9::text, $10::jsonb from new_call
         )
         select * from new_call`,
        [id, deviceId, scenario, accent, difficulty, minutes, voice, startedAt, opening, JSON.stringify(tips)],
      );
      return rows[0];
    },

    async getCall(id) {
      const { rows } = await pool.query(`select ${CALL_COLUMNS} from callready_calls where id = $1`, [id]);
      return rows[0] ?? null;
    },

    async getTurns(callId) {
      const { rows } = await pool.query(
        'select seq, speaker, text, tips, note from callready_turns where call_id = $1 order by seq',
        [callId],
      );
      return rows;
    },

    /**
     * Appends the learner's line and the caller's answer to it, after turn
     * `seq` (the last one stored). One statement: both lines or neither.
     */
    async addExchange(callId, { seq, learnerText, callerText, tips, done }) {
      await pool.query(
        `with lines as (
           insert into callready_turns (call_id, seq, speaker, text, tips) values
             ($1::text, $2::int + 1, 'learner', $3::text, null),
             ($1::text, $2::int + 2, 'caller', $4::text, $5::jsonb)
         )
         update callready_calls set caller_done = true where id = $1::text and $6::boolean`,
        [callId, seq, learnerText, callerText, JSON.stringify(tips), done],
      );
    },

    /**
     * Ends an active call. `report` is null for a call nobody answered;
     * otherwise it is stored with its notes (`{seq: note}`). Returns whether
     * this request was the one that ended it.
     */
    async endCall(callId, { endedAt, durationSeconds, report }) {
      const s = report?.skills ?? {};
      const { rows } = await pool.query(
        `with ended as (
           update callready_calls set status = 'ended', ended_at = $2::timestamptz, duration_seconds = $3::int
            where id = $1::text and status = 'active'
           returning id
         ), stored as (
           insert into callready_reports
             (call_id, score, clarity, empathy, listening, problem_solving, professional_language, did_well, try_next)
           select id, $5::int, $6::int, $7::int, $8::int, $9::int, $10::int, $11::text, $12::text
             from ended where $4::boolean
           on conflict (call_id) do nothing
         ), noted as (
           update callready_turns t set note = n.value
             from jsonb_each_text($13::jsonb) n, ended e
            where t.call_id = e.id and t.seq = n.key::int and t.speaker = 'learner'
         )
         select count(*)::int as ended from ended`,
        [
          callId, endedAt, durationSeconds, Boolean(report),
          report?.score ?? null, s.clarity ?? null, s.empathy ?? null, s.listening ?? null,
          s.problem_solving ?? null, s.professional_language ?? null,
          report?.did_well ?? null, report?.try_next ?? null,
          JSON.stringify(report?.notes ?? {}),
        ],
      );
      return rows[0].ended === 1;
    },

    async getReport(callId) {
      const { rows } = await pool.query(
        `select score, clarity, empathy, listening, problem_solving, professional_language, did_well, try_next
           from callready_reports where call_id = $1`,
        [callId],
      );
      return rows[0] ?? null;
    },

    /** Ended, scored calls, newest first. */
    async listCalls(deviceId, limit) {
      const { rows } = await pool.query(
        `select c.id, c.number, c.scenario, c.started_at, c.duration_seconds, c.minutes, r.score
           from callready_calls c join callready_reports r on r.call_id = c.id
          where c.device_id = $1 and c.status = 'ended'
          order by c.started_at desc, c.id
          limit $2`,
        [deviceId, limit],
      );
      return rows;
    },

    /**
     * Removes everything stored for test devices (ids starting `test_`); the
     * calls, turns and reports go with the profile. Used by the test suite.
     * With `deviceId` only that device is removed, and it must be a test id.
     */
    async deleteTestData(deviceId) {
      if (deviceId !== undefined) {
        if (!deviceId.startsWith('test_')) throw new Error('only test_ devices may be deleted');
        const { rowCount } = await pool.query('delete from callready_profiles where device_id = $1', [deviceId]);
        return rowCount;
      }
      const { rowCount } = await pool.query(
        `delete from callready_profiles where left(device_id, 5) = 'test_'`,
      );
      return rowCount;
    },
  };
}
