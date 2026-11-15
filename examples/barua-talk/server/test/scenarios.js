// The behaviour of the API, written once and run twice: against the
// in-memory database (`unit.test.js`) and against PostgreSQL
// (`integration.test.js`). Upstream is the fake Barua Talk in both.
//
// `open(options)` returns a harness (`fakes.js`) whose database is empty.

import assert from 'node:assert/strict';
import { test } from 'node:test';

export const A = 'device-neema-01';
export const B = 'device-baraka-02';
export const C = 'device-chausiku-3';

const ISO = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/;

/** Names the three people; returns their identities. */
export async function introduce(api) {
  const names = { [A]: 'Neema', [B]: 'Baraka', [C]: 'Chausiku' };
  for (const [device, name] of Object.entries(names)) {
    const response = await api('PUT', '/v1/me', { as: device, body: { name } });
    assert.equal(response.status, 200, JSON.stringify(response.json));
  }
  return { a: 'neema', b: 'baraka', c: 'chausiku' };
}

async function ok(promise, status = 200) {
  const response = await promise;
  assert.equal(response.status, status, JSON.stringify(response.json));
  return response.json;
}

async function fails(promise, status, code) {
  const response = await promise;
  assert.deepEqual([response.status, response.json?.error?.code], [status, code], JSON.stringify(response.json));
  assert.ok(response.json.error.message.length > 0);
  return response;
}

/**
 * `again(err)`, when given, says whether a failed scenario should be run
 * once more from the start (the integration test passes one for "the
 * database could not be reached": that is the network, not the code).
 */
export function defineScenarios(open, { again = () => false, attempts = 3, pauseMs = 5000 } = {}) {
  /** Runs `body` with a fresh harness and closes it afterwards. */
  const scenario = (name, options, body) =>
    test(name, async () => {
      for (let attempt = 1; ; attempt += 1) {
        try {
          const h = await open(options);
          try {
            await body(h);
          } finally {
            await h.close();
          }
          return;
        } catch (err) {
          if (attempt >= attempts || !again(err)) throw err;
          await new Promise((resolve) => setTimeout(resolve, pauseMs));
        }
      }
    });

  scenario('me: no identity at first, then one derived from the name and kept', {}, async ({ api }) => {
    assert.deepEqual(await ok(api('GET', '/v1/me')), { identity: null, name: null });
    // Looking at oneself gives no identity and registers nobody.
    assert.deepEqual((await ok(api('GET', '/v1/people', { as: B }))).people, []);

    assert.deepEqual(await ok(api('PUT', '/v1/me', { body: { name: '  Neema   Mushi ' } })), { identity: 'neema-mushi', name: 'Neema Mushi' });
    assert.deepEqual(await ok(api('GET', '/v1/me')), { identity: 'neema-mushi', name: 'Neema Mushi' });
    // The same name on another phone: the address stays unique.
    assert.deepEqual(await ok(api('PUT', '/v1/me', { as: C, body: { name: 'Neema Mushi' } })), { identity: 'neema-mushi-2', name: 'Neema Mushi' });
    // A new name changes the name only.
    assert.deepEqual(await ok(api('PUT', '/v1/me', { body: { name: 'Zoë Müller' } })), { identity: 'neema-mushi', name: 'Zoë Müller' });

    // B did something before naming itself (it listed people above): a guest for good.
    const guest = await ok(api('GET', '/v1/me', { as: B }));
    assert.match(guest.identity, /^guest-[0-9a-f]{6}$/);
    assert.equal(guest.name, null);
    assert.deepEqual(await ok(api('PUT', '/v1/me', { as: B, body: { name: 'Baraka' } })), { identity: guest.identity, name: 'Baraka' });

    assert.deepEqual(await ok(api('PUT', '/v1/me', { as: 'device-accent-04', body: { name: 'Zoë Müller' } })), { identity: 'zoe-muller', name: 'Zoë Müller' });
    assert.deepEqual(await ok(api('PUT', '/v1/me', { as: 'device-symbol-05', body: { name: '★' } })), { identity: 'person', name: '★' });

    await fails(api('PUT', '/v1/me', { body: {} }), 400, 'bad_request');
    await fails(api('PUT', '/v1/me', { body: { name: '   ' } }), 400, 'bad_request');
    await fails(api('PUT', '/v1/me', { body: { name: 'x'.repeat(41) } }), 400, 'bad_request');
  });

  scenario('people: the others, with a name always and online for 30 seconds', {}, async ({ api, now }) => {
    await introduce(api);
    await ok(api('GET', '/v1/rooms', { as: 'device-guest-0006' }));
    now.advance(31000);
    await ok(api('GET', '/v1/people', { as: B })); // B is seen now; the others 31 s ago
    const { people, now: stamp } = await ok(api('GET', '/v1/people'));
    assert.match(stamp, ISO);
    assert.deepEqual(Object.keys(people[0]), ['identity', 'name', 'seen_at', 'online', 'unread', 'last_message']);
    assert.deepEqual([people[0].unread, people[0].last_message], [0, null]);
    assert.deepEqual(people.map((p) => [p.identity.replace(/-[0-9a-f]{6}$/, '-x'), p.online]).sort(), [['baraka', true], ['chausiku', false], ['guest-x', false]]);
    const guest = people.find((p) => p.identity.startsWith('guest-'));
    assert.equal(guest.name, guest.identity);
    assert.ok(!people.some((p) => p.identity === 'neema'));
  });

  scenario('rooms: made upstream once, seen by everyone, ticketed, closed by their maker', {}, async ({ api, fake }) => {
    await introduce(api);
    const made = await ok(api('POST', '/v1/rooms', { body: { display_name: 'Support Desk' } }), 201);
    assert.deepEqual(Object.keys(made.room), [
      'name', 'display_name', 'created_by', 'mine', 'member', 'members', 'unread', 'last_message', 'active_at', 'live', 'recording', 'created_at',
    ]);
    assert.deepEqual(
      [made.room.name, made.room.display_name, made.room.created_by, made.room.mine, made.room.member, made.room.members, made.room.unread, made.room.last_message, made.room.live, made.room.recording],
      ['support-desk', 'Support Desk', { identity: 'neema', name: 'Neema' }, true, true, 1, 0, null, true, null],
    );
    assert.match(made.room.created_at, ISO);
    assert.deepEqual(fake.requests.at(-1), { method: 'POST', path: '/rooms', body: { name: 'support-desk', displayName: 'Support Desk' } });

    // Again, by anyone: the same room, and upstream is not asked.
    const again = await ok(api('POST', '/v1/rooms', { as: B, body: { display_name: 'support desk!' } }), 200);
    assert.deepEqual([again.room.name, again.room.display_name, again.room.mine, again.room.member], ['support-desk', 'Support Desk', false, false]);
    assert.equal(fake.count('POST', '/rooms'), 1);

    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Wateja wa Leo', name: 'wateja' } }), 201);
    await fails(api('POST', '/v1/rooms', { body: { display_name: 'x' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/rooms', { body: { display_name: '★★★' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/rooms', { body: { display_name: 'Fine', name: 'Not Fine' } }), 400, 'bad_request');

    const list = await ok(api('GET', '/v1/rooms', { as: B }));
    assert.deepEqual(list.rooms.map((r) => [r.name, r.member, r.unread]).sort(), [['support-desk', false, 0], ['wateja', false, 0]]);
    assert.equal((await ok(api('GET', '/v1/rooms/wateja', { as: B }))).room.display_name, 'Wateja wa Leo');
    await fails(api('GET', '/v1/rooms/nowhere'), 404, 'not_found');

    // A ticket: this person's, for this room; taking one makes a member.
    fake.reset();
    const ticket = await ok(api('POST', '/v1/rooms/support-desk/ticket', { as: B, body: { video: true } }));
    assert.deepEqual(Object.keys(ticket), ['token', 'url', 'identity', 'room', 'video', 'expires_at']);
    assert.deepEqual([ticket.identity, ticket.room, ticket.video, ticket.url], ['baraka', 'support-desk', true, 'wss://fake.livekit.invalid']);
    assert.ok(ticket.token.startsWith('ticket.baraka.'));
    assert.deepEqual(fake.requests, [
      { method: 'POST', path: '/rooms/support-desk/tokens', body: { identity: 'baraka', name: 'Baraka', video: true, ttlSeconds: 3600 } },
    ]);
    assert.deepEqual((await ok(api('GET', '/v1/rooms/support-desk', { as: B }))).room.members, 2);
    assert.equal((await ok(api('POST', '/v1/rooms/support-desk/ticket', {}))).video, false);
    await fails(api('POST', '/v1/rooms/nowhere/ticket', { body: {} }), 404, 'not_found');
    await fails(api('POST', '/v1/rooms/support-desk/ticket', { body: { video: 'yes' } }), 400, 'bad_request');

    // The room vanished upstream (closed from the console): a ticket puts it back.
    fake.rooms.delete('wateja');
    const repaired = await ok(api('POST', '/v1/rooms/wateja/ticket', { body: { video: true } }));
    assert.ok(repaired.token.startsWith('ticket.neema.'));
    assert.ok(fake.rooms.has('wateja'));

    // Only the maker closes; closing forgets the chat too. Both spellings.
    await ok(api('POST', '/v1/rooms/support-desk/messages', { body: { text: 'Karibu' } }), 201);
    await fails(api('DELETE', '/v1/rooms/support-desk', { as: B }), 403, 'not_yours');
    assert.deepEqual(await ok(api('DELETE', '/v1/rooms/support-desk')), { closed: true });
    assert.ok(!fake.rooms.has('support-desk'));
    await fails(api('GET', '/v1/rooms/support-desk/messages'), 404, 'not_found');
    // Already gone upstream: still closed here.
    fake.rooms.delete('wateja');
    assert.deepEqual(await ok(api('POST', '/v1/rooms/wateja/close', {})), { closed: true });
    assert.deepEqual((await ok(api('GET', '/v1/rooms'))).rooms, []);
    // The name is free again, and the new room starts empty.
    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Support desk' } }), 201);
    assert.deepEqual((await ok(api('GET', '/v1/rooms/support-desk/messages'))).messages, []);
  });

  scenario('chat: stored here, forwarded upstream, read marks and polling', {}, async ({ api, fake, now }) => {
    await introduce(api);
    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }), 201);
    assert.deepEqual(await ok(api('GET', '/v1/rooms/desk/messages')), {
      messages: [], last_id: 0, more: false, unread: 0, now: '2026-11-15T07:00:00Z',
    });

    now.advance(1000);
    const first = (await ok(api('POST', '/v1/rooms/desk/messages', { body: { text: '  Habari za asubuhi\nwote  ' } }), 201)).message;
    assert.deepEqual(Object.keys(first), ['id', 'from', 'from_name', 'mine', 'text', 'created_at', 'delivered']);
    assert.deepEqual(
      [first.from, first.from_name, first.mine, first.text, first.created_at, first.delivered],
      ['neema', 'Neema', true, 'Habari za asubuhi\nwote', '2026-11-15T07:00:01Z', true],
    );
    assert.ok(Number.isInteger(first.id) && first.id > 0);
    // LiveKit clients in the room got it too, from the author's identity.
    assert.deepEqual(fake.chat.get('desk').map((m) => [m.from, m.text]), [['neema', 'Habari za asubuhi\nwote']]);

    now.advance(1000);
    const second = (await ok(api('POST', '/v1/rooms/desk/messages', { as: B, body: { text: 'Nzuri!' } }), 201)).message;
    assert.ok(second.id > first.id);
    const third = (await ok(api('POST', '/v1/rooms/desk/messages', { as: B, body: { text: 'Karibu' } }), 201)).message;

    // The author's own messages are never unread; the other's are.
    let room = (await ok(api('GET', '/v1/rooms/desk'))).room;
    assert.deepEqual([room.unread, room.members, room.last_message.text, room.last_message.mine, room.active_at], [2, 2, 'Karibu', false, '2026-11-15T07:00:02Z']);
    assert.equal((await ok(api('GET', '/v1/rooms/desk', { as: B }))).room.unread, 0);
    // Someone who has not opened the room is no member and has no unread.
    assert.deepEqual((await ok(api('GET', '/v1/rooms/desk', { as: C }))).room.unread, 0);

    // Opening the screen: everything, oldest first. Looking does not mark read…
    const opened = await ok(api('GET', '/v1/rooms/desk/messages'));
    assert.deepEqual(opened.messages.map((m) => [m.id, m.mine]), [[first.id, true], [second.id, false], [third.id, false]]);
    assert.deepEqual([opened.last_id, opened.more, opened.unread], [third.id, false, 2]);
    // …the mark does, forwards only, and never beyond the newest message.
    assert.deepEqual(await ok(api('POST', '/v1/rooms/desk/read', { body: { up_to: second.id } })), { unread: 1 });
    assert.deepEqual(await ok(api('POST', '/v1/rooms/desk/read', { body: { up_to: first.id } })), { unread: 1 });
    assert.deepEqual(await ok(api('POST', '/v1/rooms/desk/read', { body: { up_to: 999999 } })), { unread: 0 });
    const fourth = (await ok(api('POST', '/v1/rooms/desk/messages', { as: B, body: { text: 'Bado?' } }), 201)).message;
    assert.equal((await ok(api('GET', '/v1/rooms/desk'))).room.unread, 1);
    assert.deepEqual(await ok(api('POST', '/v1/rooms/desk/read', {})), { unread: 0 });
    await fails(api('POST', '/v1/rooms/desk/read', { body: { up_to: 'x' } }), 400, 'bad_request');

    // Polling: only what is new, pages of `limit`, `more` while there is more.
    const page = await ok(api('GET', `/v1/rooms/desk/messages?after=${first.id}&limit=2`));
    assert.deepEqual([page.messages.map((m) => m.id), page.more, page.last_id], [[second.id, third.id], true, fourth.id]);
    const rest = await ok(api('GET', `/v1/rooms/desk/messages?after=${third.id}&limit=2`));
    assert.deepEqual([rest.messages.map((m) => m.id), rest.more], [[fourth.id], false]);
    assert.deepEqual((await ok(api('GET', `/v1/rooms/desk/messages?after=${fourth.id}`))).messages, []);
    assert.deepEqual((await ok(api('GET', '/v1/rooms/desk/messages?limit=2'))).messages.map((m) => m.id), [third.id, fourth.id]);
    await fails(api('GET', '/v1/rooms/desk/messages?after=abc'), 400, 'bad_request');
    await fails(api('GET', '/v1/rooms/desk/messages?limit=0'), 400, 'bad_request');

    // `read=1` moves the mark to the newest message returned. C opens the room.
    assert.equal((await ok(api('GET', '/v1/rooms/desk/messages', { as: C }))).unread, 4);
    assert.equal((await ok(api('GET', '/v1/rooms/desk/messages?read=1', { as: C }))).unread, 0);
    assert.equal((await ok(api('GET', '/v1/rooms/desk', { as: C }))).room.members, 3);

    // The same `client_id` again: the first message, stored and forwarded once.
    fake.reset();
    const sent = await ok(api('POST', '/v1/rooms/desk/messages', { body: { text: 'Mara moja', client_id: 'm-00000001' } }), 201);
    const resent = await ok(api('POST', '/v1/rooms/desk/messages', { body: { text: 'Mara moja', client_id: 'm-00000001' } }), 200);
    assert.deepEqual(resent.message, sent.message);
    assert.equal(fake.count('POST', '/rooms/desk/chat'), 1);
    assert.equal((await ok(api('GET', '/v1/rooms/desk/messages'))).messages.filter((m) => m.text === 'Mara moja').length, 1);
    // Another person may use the same client id.
    await ok(api('POST', '/v1/rooms/desk/messages', { as: B, body: { text: 'Yangu', client_id: 'm-00000001' } }), 201);

    await fails(api('POST', '/v1/rooms/desk/messages', { body: { text: '   ' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/rooms/desk/messages', { body: { text: 'x'.repeat(2001) } }), 400, 'bad_request');
    await fails(api('POST', '/v1/rooms/desk/messages', { body: { text: 'ok', client_id: 'short' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/rooms/nowhere/messages', { body: { text: 'ok' } }), 404, 'not_found');
  });

  scenario('chat: an upstream failure does not lose the message', {}, async ({ api, fake }) => {
    await introduce(api);
    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }), 201);
    for (const how of [{ status: 500, body: '' }, 'drop-before', { status: 404, body: { error: { code: 'room_not_found', message: 'No room.' } } }]) {
      fake.fail('POST', '/rooms/desk/chat', how);
      const { message } = await ok(api('POST', '/v1/rooms/desk/messages', { body: { text: 'Bado nipo' } }), 201);
      assert.equal(message.delivered, false);
    }
    const stored = (await ok(api('GET', '/v1/rooms/desk/messages', { as: B }))).messages;
    assert.deepEqual(stored.map((m) => [m.text, m.delivered]), [['Bado nipo', false], ['Bado nipo', false], ['Bado nipo', false]]);
    // One try each: nothing was sent a second time.
    assert.equal(fake.count('POST', '/rooms/desk/chat'), 3);
    assert.deepEqual(fake.chat.get('desk'), []);
  });

  scenario('messages between two people: kept here, read marks, only the two can read them', {}, async ({ api, fake, now }) => {
    await introduce(api);
    const upstreamBefore = fake.requests.length;
    assert.deepEqual(await ok(api('GET', '/v1/people/baraka/messages')), {
      peer: { identity: 'baraka', name: 'Baraka', online: true },
      messages: [], last_id: 0, more: false, unread: 0, now: '2026-11-15T07:00:00Z',
    });

    now.advance(1000);
    const first = (await ok(api('POST', '/v1/people/baraka/messages', { body: { text: '  Habari\nBaraka  ', client_id: 'direct-0001' } }), 201)).message;
    assert.deepEqual(Object.keys(first), ['id', 'from', 'from_name', 'mine', 'text', 'created_at', 'delivered']);
    assert.deepEqual(
      [first.from, first.from_name, first.mine, first.text, first.created_at, first.delivered],
      ['neema', 'Neema', true, 'Habari\nBaraka', '2026-11-15T07:00:01Z', true],
    );
    // The same client id again: the first message, nothing new.
    const again = await api('POST', '/v1/people/baraka/messages', { body: { text: 'Habari\nBaraka', client_id: 'direct-0001' } });
    assert.deepEqual([again.status, again.json.message.id], [200, first.id]);
    // …and not for a message to someone else.
    await fails(api('POST', '/v1/people/chausiku/messages', { body: { text: 'Habari', client_id: 'direct-0001' } }), 409, 'client_id_used');

    now.advance(1000);
    const second = (await ok(api('POST', '/v1/people/neema/messages', { as: B, body: { text: 'Nzuri!' } }), 201)).message;
    const third = (await ok(api('POST', '/v1/people/neema/messages', { as: B, body: { text: 'Karibu   sana' } }), 201)).message;
    assert.ok(second.id > first.id && third.id > second.id);

    // The author's own messages are never unread; the other's are, and the people list says so.
    let people = (await ok(api('GET', '/v1/people'))).people;
    const baraka = people.find((p) => p.identity === 'baraka');
    assert.deepEqual([baraka.unread, baraka.last_message.text, baraka.last_message.mine, baraka.last_message.from_name], [2, 'Karibu   sana', false, 'Baraka']);
    assert.deepEqual(people.find((p) => p.identity === 'chausiku').last_message, null);
    people = (await ok(api('GET', '/v1/people', { as: B }))).people;
    assert.deepEqual([people.find((p) => p.identity === 'neema').unread, people.find((p) => p.identity === 'neema').last_message.mine], [0, true]);

    // Looking does not mark read; `read=1` does, up to what was returned.
    const opened = await ok(api('GET', '/v1/people/baraka/messages'));
    assert.deepEqual(opened.messages.map((m) => [m.id, m.mine]), [[first.id, true], [second.id, false], [third.id, false]]);
    assert.deepEqual([opened.last_id, opened.more, opened.unread], [third.id, false, 2]);
    const page = await ok(api('GET', `/v1/people/baraka/messages?after=${first.id}&limit=1&read=1`));
    assert.deepEqual([page.messages.map((m) => m.id), page.more, page.unread], [[second.id], true, 1]);
    assert.deepEqual(await ok(api('POST', '/v1/people/baraka/read', { body: {} })), { unread: 0 });
    // A mark never moves back.
    assert.deepEqual(await ok(api('POST', '/v1/people/baraka/read', { body: { up_to: 0 } })), { unread: 0 });
    const quiet = await ok(api('GET', `/v1/people/baraka/messages?after=${third.id}`));
    assert.deepEqual([quiet.messages, quiet.last_id, quiet.unread], [[], third.id, 0]);

    // The conversation is in the activity list, for both, and for nobody else.
    const mine = (await ok(api('GET', '/v1/activity'))).items;
    assert.deepEqual(mine, [{
      kind: 'person', id: 'baraka', title: 'Baraka', preview: 'Karibu sana', at: '2026-11-15T07:00:02Z',
      unread: 0, live: false, status: 'open', direction: null, video: null, seconds: null, peer: { identity: 'baraka', name: 'Baraka' },
    }]);
    assert.deepEqual((await ok(api('GET', '/v1/activity', { as: B }))).items.map((i) => [i.kind, i.id, i.preview]), [['person', 'neema', 'You: Karibu sana']]);
    assert.deepEqual((await ok(api('GET', '/v1/activity', { as: C }))).items, []);
    // A third person asking for "their" conversation with either sees their own, empty one.
    assert.deepEqual((await ok(api('GET', '/v1/people/neema/messages', { as: C }))).messages, []);
    assert.deepEqual((await ok(api('GET', '/v1/people/baraka/messages', { as: C }))).messages, []);

    // Refusals.
    await fails(api('GET', '/v1/people/nobody/messages'), 404, 'not_found');
    await fails(api('POST', '/v1/people/nobody/messages', { body: { text: 'x' } }), 404, 'not_found');
    await fails(api('POST', '/v1/people/neema/messages', { body: { text: 'to myself' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/people/baraka/messages', { body: { text: '   ' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/people/baraka/messages', { body: { text: 'x'.repeat(2001) } }), 400, 'bad_request');
    await fails(api('POST', '/v1/people/baraka/messages', { body: { text: 'x', client_id: 'short' } }), 400, 'bad_request');
    await fails(api('GET', '/v1/people/baraka/messages?after=-1'), 400, 'bad_request');
    await fails(api('POST', '/v1/people/baraka/read', { body: { up_to: 'x' } }), 400, 'bad_request');
    // Nothing of this went to Barua Talk.
    assert.equal(fake.requests.length, upstreamBefore);
  });

  scenario('without a Barua key: 503 where upstream is needed, the rest works', { key: false, env: { PHONE_CALLS: 'on' } }, async ({ api, fake, db }) => {
    assert.deepEqual(await ok(api('GET', '/v1/health', { as: null })), { ok: false, talk: false, database: true, phone: false });
    const ids = await introduce(api);
    assert.equal((await ok(api('GET', '/v1/people'))).people.length, 2);
    await fails(api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }), 503, 'talk_unavailable');
    assert.deepEqual((await ok(api('GET', '/v1/rooms'))).rooms, []);

    // A room that is already here (made while there was a key) still chats.
    await db.createRoom({ name: 'desk', displayName: 'Desk', createdBy: ids.a, at: new Date('2026-11-15T06:00:00Z') });
    const { message } = await ok(api('POST', '/v1/rooms/desk/messages', { body: { text: 'Bila ufunguo' } }), 201);
    assert.equal(message.delivered, false);
    assert.equal((await ok(api('GET', '/v1/rooms/desk/messages', { as: B }))).messages[0].text, 'Bila ufunguo');
    assert.equal((await ok(api('GET', '/v1/activity'))).items[0].preview, 'You: Bila ufunguo');

    await fails(api('POST', '/v1/rooms/desk/ticket', { body: {} }), 503, 'talk_unavailable');
    await fails(api('DELETE', '/v1/rooms/desk'), 503, 'talk_unavailable');
    await fails(api('POST', '/v1/calls', { body: { to: ids.b } }), 503, 'talk_unavailable');
    await fails(api('POST', '/v1/phone-calls', { body: { to: '+255712345678' } }), 503, 'talk_unavailable');
    await fails(api('POST', '/v1/recordings', { body: { room: 'desk' } }), 503, 'talk_unavailable');
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming'))).calls, []);
    assert.equal((await ok(api('GET', '/v1/account'))).phone.enabled, false);
    assert.equal(fake.requests.length, 0);
  });

  scenario('in-app call: ring, see it ringing, accept, talk, hang up', {}, async ({ api, fake, now }) => {
    const ids = await introduce(api);
    await fails(api('POST', '/v1/calls', { body: { to: 'nobody' } }), 404, 'not_found');
    await fails(api('POST', '/v1/calls', { body: { to: ids.a } }), 400, 'bad_request');
    await fails(api('POST', '/v1/calls', { body: {} }), 400, 'bad_request');
    await fails(api('POST', '/v1/calls', { body: { to: ids.b, video: 1 } }), 400, 'bad_request');
    assert.equal(fake.requests.length, 0);

    const rung = await ok(api('POST', '/v1/calls', { body: { to: ids.b, video: true } }), 201);
    assert.deepEqual(Object.keys(rung), ['call', 'token', 'url']);
    assert.deepEqual(Object.keys(rung.call), ['id', 'direction', 'video', 'status', 'peer', 'created_at', 'answered_at', 'ended_at', 'seconds', 'recording']);
    assert.deepEqual(
      [rung.call.direction, rung.call.video, rung.call.status, rung.call.peer, rung.call.answered_at, rung.call.seconds, rung.call.recording],
      ['outgoing', true, 'ringing', { identity: 'baraka', name: 'Baraka' }, null, 0, null],
    );
    assert.ok(rung.token.startsWith('ticket.neema.'));
    assert.deepEqual(fake.requests.at(-1).body, { from: 'neema', to: 'baraka', video: true });
    const path = `/v1/calls/${rung.call.id}`;

    // It rings for B, and only for B. Polling costs upstream nothing yet.
    fake.reset();
    for (let i = 0; i < 5; i += 1) {
      const { calls } = await ok(api('GET', '/v1/calls/incoming', { as: B }));
      assert.deepEqual(calls.map((c) => [c.id, c.direction, c.status, c.peer]), [[rung.call.id, 'incoming', 'ringing', { identity: 'neema', name: 'Neema' }]]);
      assert.equal((await ok(api('GET', path))).call.status, 'ringing');
    }
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming'))).calls, []);
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: C }))).calls, []);
    assert.equal(fake.requests.length, 0);

    // Only its two people see it; only the callee answers it.
    await fails(api('GET', path, { as: C }), 404, 'not_found');
    await fails(api('POST', `${path}/accept`, { as: C }), 404, 'not_found');
    await fails(api('POST', `${path}/accept`), 403, 'not_your_call');
    await fails(api('GET', '/v1/calls/unknown'), 404, 'not_found');

    now.advance(3000);
    const accepted = await ok(api('POST', `${path}/accept`, { as: B }));
    assert.deepEqual([accepted.call.status, accepted.call.direction, accepted.call.answered_at], ['active', 'incoming', '2026-11-15T07:00:03Z']);
    assert.ok(accepted.token.startsWith('ticket.baraka.'));
    assert.deepEqual(fake.requests.at(-1), { method: 'POST', path: `/app-calls/${rung.call.id}`, body: { action: 'accept', identity: 'baraka' } });

    // The caller learns it at once, with no upstream request; the timer runs here.
    fake.reset();
    now.advance(4000);
    const watched = await ok(api('GET', path));
    assert.deepEqual([watched.call.status, watched.call.seconds, watched.now], ['active', 4, '2026-11-15T07:00:07Z']);
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);
    assert.equal(fake.requests.length, 0);
    await fails(api('POST', `${path}/accept`, { as: B }), 409, 'not_ringing');
    await fails(api('POST', `${path}/decline`, { as: B }), 409, 'not_ringing');

    // Either side hangs up; upstream says how long it was. Twice is harmless.
    const over = await ok(api('DELETE', path, { as: B }));
    assert.deepEqual([over.call.status, over.call.seconds, over.call.ended_at !== null], ['ended', 7, true]);
    const twice = await ok(api('POST', `${path}/hangup`, {}));
    assert.deepEqual([twice.call.status, twice.call.seconds, twice.call.direction], ['ended', 7, 'outgoing']);
    assert.equal(fake.count('DELETE', `/app-calls/${rung.call.id}`), 1);

    const mine = await ok(api('GET', '/v1/calls'));
    assert.deepEqual(mine.calls.map((c) => [c.id, c.status]), [[rung.call.id, 'ended']]);
    assert.deepEqual((await ok(api('GET', '/v1/calls', { as: C }))).calls, []);
  });

  scenario('in-app call: declined, given up, and ended by the platform', {}, async ({ api, fake, now, config }) => {
    const ids = await introduce(api);
    const ring = async () => (await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201)).call;

    // Declined.
    const first = await ring();
    const declined = await ok(api('POST', `/v1/calls/${first.id}/decline`, { as: B }));
    assert.deepEqual([declined.call.status, declined.call.video], ['declined', false]);
    assert.equal((await ok(api('GET', `/v1/calls/${first.id}`))).call.status, 'declined');

    // The caller gives up: missed, and it stops ringing.
    now.advance(1000);
    const second = await ring();
    assert.equal((await ok(api('DELETE', `/v1/calls/${second.id}`))).call.status, 'missed');
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);

    // The caller's app just closes. Upstream notices; this server learns it
    // when the time-to-live has passed, with one request for everyone asking.
    now.advance(1000);
    const third = await ring();
    fake.abandon(third.id);
    fake.reset();
    assert.equal((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls.length, 1); // still fresh enough
    now.advance(config.talk.ringingTtlMs);
    const polls = await Promise.all([
      api('GET', '/v1/calls/incoming', { as: B }),
      api('GET', '/v1/calls/incoming', { as: B }),
      api('GET', `/v1/calls/${third.id}`),
      api('GET', `/v1/calls/${third.id}`, { as: B }),
    ]);
    assert.deepEqual(polls.map((p) => p.status), [200, 200, 200, 200]);
    assert.equal((await ok(api('GET', `/v1/calls/${third.id}`))).call.status, 'missed');
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);
    assert.equal(fake.count('GET', `/app-calls/${third.id}`), 1);
    // Too late to answer.
    await fails(api('POST', `/v1/calls/${third.id}/accept`, { as: B }), 409, 'not_ringing');

    // The same, before this server has noticed: upstream says not_ringing,
    // and the next look shows why.
    now.advance(1000);
    const fourth = await ring();
    fake.abandon(fourth.id);
    await fails(api('POST', `/v1/calls/${fourth.id}/accept`, { as: B }), 409, 'not_ringing');
    assert.equal((await ok(api('GET', `/v1/calls/${fourth.id}`, { as: B }))).call.status, 'missed');

    // An answered call whose people both vanish is ended the same way.
    now.advance(1000);
    const fifth = await ring();
    await ok(api('POST', `/v1/calls/${fifth.id}/accept`, { as: B }));
    fake.abandon(fifth.id);
    now.advance(config.talk.activeTtlMs);
    const ended = (await ok(api('GET', `/v1/calls/${fifth.id}`))).call;
    assert.deepEqual([ended.status, ended.seconds], ['ended', 42]);

    const history = (await ok(api('GET', '/v1/calls', { as: B }))).calls;
    assert.deepEqual(history.map((c) => [c.status, c.direction]), [
      ['ended', 'incoming'], ['missed', 'incoming'], ['missed', 'incoming'], ['missed', 'incoming'], ['declined', 'incoming'],
    ]);
  });

  scenario('in-app call: what polling costs, and what happens when upstream cannot be asked', {}, async ({ api, fake, now, config }) => {
    const ids = await introduce(api);
    const { call } = await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201);
    fake.reset();
    // A minute of both phones polling every two seconds…
    for (let second = 2; second <= 60; second += 2) {
      now.advance(2000);
      if (second === 44) break; // …stopping short of the ring timeout
      await ok(api('GET', '/v1/calls/incoming', { as: B }));
      await ok(api('GET', `/v1/calls/${call.id}`));
    }
    // …is one upstream request every five seconds, not one per poll.
    const asked = fake.count('GET', `/app-calls/${call.id}`);
    assert.ok(asked >= 7 && asked <= 9, `asked upstream ${asked} times`);
    assert.equal(fake.requests.length, asked);

    // Upstream is unreachable: the poll is answered with what the server
    // has, and the next poll does not try again straight away.
    await ok(api('POST', `/v1/calls/${call.id}/accept`, { as: B }));
    fake.reset();
    now.advance(config.talk.activeTtlMs);
    fake.fail('GET', `/app-calls/${call.id}`, 'drop-before');
    assert.equal((await ok(api('GET', `/v1/calls/${call.id}`))).call.status, 'active');
    assert.equal((await ok(api('GET', `/v1/calls/${call.id}`))).call.status, 'active');
    assert.equal(fake.count('GET', `/app-calls/${call.id}`), 1);

    // Upstream has forgotten the call altogether: it is over.
    now.advance(config.talk.activeTtlMs);
    fake.appCalls.delete(call.id);
    assert.equal((await ok(api('GET', `/v1/calls/${call.id}`))).call.status, 'ended');
  });

  scenario('in-app call: an unanswered ring is ended after 45 seconds', {}, async ({ api, fake, now, app }) => {
    const ids = await introduce(api);
    const first = (await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201)).call;
    now.advance(44000);
    assert.equal((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls.length, 1);
    now.advance(1000);
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);
    assert.equal((await ok(api('GET', `/v1/calls/${first.id}`))).call.status, 'missed');
    assert.equal(fake.appCalls.get(first.id).status, 'missed'); // upstream was told
    assert.equal(fake.count('DELETE', `/app-calls/${first.id}`), 1);

    // With nobody polling at all, the housekeeping does it.
    const second = (await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201)).call;
    now.advance(46000);
    await app.routes.sweep();
    assert.equal(fake.appCalls.get(second.id).status, 'missed');
    assert.equal((await ok(api('GET', '/v1/activity', { as: B }))).items[0].status, 'missed');

    // Upstream cannot be told: the ring still stops here.
    const third = (await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201)).call;
    now.advance(46000);
    fake.fail('DELETE', `/app-calls/${third.id}`, 'drop-before');
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);
  });

  scenario('in-app call: a ring upstream did not confirm stores nothing', {}, async ({ api, fake }) => {
    const ids = await introduce(api);
    fake.fail('POST', '/app-calls', { status: 500, body: '' });
    await fails(api('POST', '/v1/calls', { body: { to: ids.b } }), 502, 'upstream_failed');
    fake.fail('POST', '/app-calls', { status: 201, body: { call: { status: 'ringing' } } });
    await fails(api('POST', '/v1/calls', { body: { to: ids.b } }), 502, 'upstream_failed');
    assert.deepEqual((await ok(api('GET', '/v1/calls'))).calls, []);
    assert.deepEqual((await ok(api('GET', '/v1/calls/incoming', { as: B }))).calls, []);
  });

  scenario('phone calls are off unless switched on', {}, async ({ api, fake }) => {
    await introduce(api);
    assert.equal((await ok(api('GET', '/v1/health', { as: null }))).phone, false);
    await fails(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } }), 503, 'phone_unavailable');
    assert.equal(fake.requests.length, 0);
    assert.deepEqual((await ok(api('GET', '/v1/phone-calls'))).calls, []);
  });

  scenario('phone call: dialled once, watched through one shared list, priced at the end', { env: { PHONE_CALLS: 'on' } }, async ({ api, fake, now, config }) => {
    await introduce(api);
    assert.equal((await ok(api('GET', '/v1/health', { as: null }))).phone, true);
    assert.deepEqual(await ok(api('GET', '/v1/account')), {
      phone: { enabled: true, caller_id: null },
      rates: {
        phone_call: { tzs: 300, per: 'minute', text: 'TSh 300 / minute' },
        recording: { tzs: 30, per: 'minute', text: 'TSh 30 / minute' },
      },
      balance: { state: 'unknown', tzs: null },
      spent_tzs: 0,
    });
    for (const to of ['0712345678', '+2557', '+255 712 345 678', 255712345678]) {
      await fails(api('POST', '/v1/phone-calls', { body: { to } }), 400, 'bad_request');
    }
    await fails(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'x' } }), 400, 'bad_request');
    assert.equal(fake.requests.length, 0);

    const dialled = await ok(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } }), 201);
    assert.deepEqual(Object.keys(dialled), ['call', 'token', 'url']);
    assert.deepEqual(Object.keys(dialled.call), ['id', 'to', 'caller_id', 'status', 'created_at', 'started_at', 'ended_at', 'seconds', 'price_tzs', 'rate', 'can_hang_up']);
    assert.deepEqual(
      [dialled.call.to, dialled.call.caller_id, dialled.call.status, dialled.call.seconds, dialled.call.price_tzs, dialled.call.rate, dialled.call.can_hang_up],
      ['+255712345678', '+255800110011', 'ringing', 0, null, { tzs: 300, per: 'minute', text: 'TSh 300 / minute' }, false],
    );
    assert.ok(dialled.token.startsWith('ticket.neema.'));
    assert.deepEqual(fake.requests, [{ method: 'POST', path: '/calls', body: { to: '+255712345678', identity: 'neema' } }]);
    const path = `/v1/phone-calls/${dialled.call.id}`;

    // The same press again (a retry after a timeout): the same call, no second dial.
    const repeated = await ok(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } }), 200);
    assert.deepEqual([repeated.call.id, repeated.token], [dialled.call.id, null]);
    // The header spelling of the same thing.
    const byHeader = await ok(api('POST', '/v1/phone-calls', { body: { to: '+255712345678' }, headers: { 'Idempotency-Key': 'dial-0001' } }), 200);
    assert.equal(byHeader.call.id, dialled.call.id);
    // A second call while the first has not ended: refused, not dialled.
    await fails(api('POST', '/v1/phone-calls', { body: { to: '+255700000001', request_id: 'dial-0002' } }), 409, 'call_in_progress');
    await fails(api('POST', '/v1/phone-calls', { body: { to: '+255700000001' } }), 409, 'call_in_progress');
    assert.equal(fake.count('POST', '/calls'), 1);
    assert.equal(fake.phoneCalls.length, 1);
    // Someone else may call meanwhile.
    now.advance(1000);
    const other = await ok(api('POST', '/v1/phone-calls', { as: B, body: { to: '+255700000002', request_id: 'dial-0001' } }), 201);
    assert.notEqual(other.call.id, dialled.call.id);

    // Watching: every poller shares one `GET /calls` per time-to-live.
    await fails(api('GET', path, { as: B }), 404, 'not_found');
    fake.reset();
    fake.answerPhone(fake.phoneCalls[0].id);
    now.advance(config.talk.phoneTtlMs);
    const polls = await Promise.all([api('GET', path), api('GET', path), api('GET', `/v1/phone-calls/${other.call.id}`, { as: B }), api('GET', '/v1/phone-calls')]);
    assert.deepEqual(polls.map((p) => p.status), [200, 200, 200, 200]);
    for (let i = 0; i < 5; i += 1) assert.equal((await ok(api('GET', path))).call.status, 'active');
    assert.deepEqual(fake.requests, [{ method: 'GET', path: '/calls', body: {} }]);

    // No hang-up upstream: the server says so and sends nothing.
    fake.reset();
    const left = await ok(api('DELETE', path));
    assert.deepEqual([left.hung_up, left.message, left.call.status], [false, 'The call ends when the other side hangs up.', 'active']);
    assert.equal(fake.requests.length, 0);

    // The other side hangs up: seconds and price arrive with the next look.
    fake.endPhone(fake.phoneCalls[0].id, 125);
    now.advance(config.talk.phoneTtlMs);
    const ended = (await ok(api('GET', path))).call;
    assert.deepEqual([ended.status, ended.seconds, ended.price_tzs, ended.ended_at !== null, ended.started_at !== null], ['ended', 125, 900, true, true]);
    const after = await ok(api('POST', `${path}/hangup`, {}));
    assert.deepEqual([after.hung_up, after.message], [false, 'The call is already over.']);

    // Once everything has ended the list is not fetched any more.
    fake.endPhone(fake.phoneCalls[1].id, 10);
    now.advance(config.talk.phoneTtlMs);
    await ok(api('GET', `/v1/phone-calls/${other.call.id}`, { as: B }));
    fake.reset();
    now.advance(60000);
    assert.deepEqual((await ok(api('GET', '/v1/phone-calls'))).calls.map((c) => [c.id, c.status]), [[dialled.call.id, 'ended']]);
    await ok(api('GET', path));
    assert.equal(fake.requests.length, 0);

    const account = await ok(api('GET', '/v1/account', { as: C }));
    assert.deepEqual([account.phone.caller_id, account.balance, account.spent_tzs], ['+255800110011', { state: 'ok', tzs: null }, 1200]);
    // Now a new call is allowed.
    await ok(api('POST', '/v1/phone-calls', { body: { to: '+255700000003', request_id: 'dial-0003' } }), 201);
  });

  scenario('phone call: refusals dial nothing; an unconfirmed dial is never repeated', { env: { PHONE_CALLS: 'on' } }, async ({ api, fake }) => {
    await introduce(api);
    const dial = (request_id) => api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id } });

    fake.balanceTzs = 0;
    const empty = await fails(dial('dial-0001'), 402, 'balance_empty');
    assert.equal(empty.json.error.message, 'The workspace balance is empty. Top up to carry on.');
    assert.equal((await ok(api('GET', '/v1/account'))).balance.state, 'empty');
    fake.balanceTzs = 10000;

    fake.voiceNumbers = [];
    await fails(dial('dial-0002'), 409, 'no_caller_number');
    assert.equal((await ok(api('GET', '/v1/health', { as: null }))).phone, false);
    fake.voiceNumbers = ['+255800110011'];

    fake.phoneConnected = false;
    await fails(dial('dial-0003'), 503, 'not_connected');
    fake.phoneConnected = true;

    fake.fail('POST', '/calls', { status: 429, body: { error: { code: 'rate_limited', message: 'Over 60.' } } });
    const busy = await fails(dial('dial-0004'), 429, 'talk_busy');
    assert.ok(Number(busy.headers.get('retry-after')) >= 1);

    // Nothing was dialled, nothing is left behind, and each refusal was one request.
    assert.equal(fake.phoneCalls.length, 0);
    assert.equal(fake.count('POST', '/calls'), 4);
    assert.deepEqual((await ok(api('GET', '/v1/phone-calls'))).calls, []);
  });

  scenario('phone call: a dial with no answer from upstream may have rung, so it is not dialled again', { env: { PHONE_CALLS: 'on' } }, async ({ api, fake }) => {
    await introduce(api);
    const dial = (request_id) => api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id } });
    // Upstream dials, then the connection is lost before the answer.
    fake.fail('POST', '/calls', 'drop');
    const unsure = await fails(dial('dial-0001'), 502, 'upstream_failed');
    assert.match(unsure.json.error.message, /may or may not/);
    assert.equal(fake.phoneCalls.length, 1);
    // The same press again does not dial: it shows the attempt, as failed.
    const again = await ok(dial('dial-0001'), 200);
    assert.equal(again.call.status, 'failed');
    assert.equal(fake.count('POST', '/calls'), 1);
    assert.equal(fake.phoneCalls.length, 1);
    assert.deepEqual((await ok(api('GET', '/v1/phone-calls'))).calls.map((c) => c.status), ['failed']);
    assert.equal((await ok(api('GET', '/v1/activity'))).items[0].preview, 'Call failed');

    // A bare 500 is the same: no conclusion can be drawn from it.
    fake.fail('POST', '/calls', { status: 500, body: '' });
    await fails(dial('dial-0002'), 502, 'upstream_failed');
    // A deliberate new press does dial.
    await ok(dial('dial-0003'), 201);
    assert.equal(fake.count('POST', '/calls'), 3);
  });

  scenario('phone call: hang-up with an upstream that has one', { env: { PHONE_CALLS: 'on', BARUA_PHONE_HANGUP: 'delete', BARUA_CALLER_ID: '+255800110022' } }, async ({ api, fake }) => {
    await introduce(api);
    fake.phoneHangup = true;
    assert.equal((await ok(api('GET', '/v1/account'))).phone.caller_id, '+255800110022');
    const { call } = await ok(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } }), 201);
    assert.deepEqual([call.can_hang_up, call.caller_id], [true, '+255800110022']);
    assert.equal(fake.requests.at(-1).body.from, '+255800110022');
    const over = await ok(api('DELETE', `/v1/phone-calls/${call.id}`));
    assert.deepEqual([over.hung_up, over.call.status, over.call.seconds, over.call.price_tzs, 'message' in over], [true, 'ended', 30, 300, false]);
    assert.equal(fake.count('DELETE', /^\/calls\//), 1);
  });

  scenario('recording a room: started once, stopped, ready with a link', {}, async ({ api, fake, now, config }) => {
    await introduce(api);
    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }), 201);
    await fails(api('POST', '/v1/recordings', { body: {} }), 400, 'bad_request');
    await fails(api('POST', '/v1/recordings', { body: { room: 'desk', call: 'x' } }), 400, 'bad_request');
    await fails(api('POST', '/v1/recordings', { body: { room: 'nowhere' } }), 404, 'not_found');

    fake.reset();
    const started = await ok(api('POST', '/v1/recordings', { body: { room: 'desk' } }), 201);
    assert.deepEqual(Object.keys(started.recording), ['id', 'room', 'call', 'status', 'started_by', 'created_at', 'ready_at', 'seconds', 'size_bytes', 'price_tzs', 'url', 'rate']);
    assert.deepEqual(
      [started.recording.room, started.recording.call, started.recording.status, started.recording.started_by, started.recording.url, started.recording.rate],
      ['desk', null, 'recording', { identity: 'neema', name: 'Neema' }, null, { tzs: 30, per: 'minute', text: 'TSh 30 / minute' }],
    );
    assert.deepEqual(fake.requests, [{ method: 'POST', path: '/recordings', body: { room: 'desk' } }]);
    const path = `/v1/recordings/${started.recording.id}`;

    // Pressed again, by anyone: the running recording, not a second one.
    assert.equal((await ok(api('POST', '/v1/recordings', { body: { room: 'desk' } }), 200)).recording.id, started.recording.id);
    assert.equal((await ok(api('POST', '/v1/recordings', { as: B, body: { room: 'desk' } }), 200)).recording.id, started.recording.id);
    assert.equal(fake.count('POST', '/recordings'), 1);
    assert.equal(fake.recordings.size, 1);

    // The room says it is being recorded; the timer runs here.
    now.advance(20000);
    assert.deepEqual((await ok(api('GET', '/v1/rooms/desk'))).room.recording, { id: started.recording.id, status: 'recording' });
    assert.equal((await ok(api('GET', path))).recording.seconds, 20);
    // Members of the room see it; others do not.
    assert.equal((await ok(api('GET', path, { as: B }))).recording.status, 'recording');
    await fails(api('GET', path, { as: C }), 404, 'not_found');
    await fails(api('POST', `${path}/stop`, { as: C }), 404, 'not_found');
    await fails(api('GET', '/v1/recordings?room=desk', { as: C }), 404, 'not_found');

    const stopped = await ok(api('DELETE', path, { as: B }));
    assert.equal(stopped.recording.status, 'processing');
    assert.equal((await ok(api('POST', `${path}/stop`, {}))).recording.status, 'processing'); // again: as it is
    assert.equal(fake.count('DELETE', /^\/recordings\//), 1);
    assert.equal((await ok(api('GET', '/v1/rooms/desk'))).room.recording, null);

    // Processing is polled upstream at most once per time-to-live.
    const upstreamId = [...fake.recordings.keys()][0];
    fake.reset();
    for (let i = 0; i < 4; i += 1) await ok(api('GET', path));
    assert.equal(fake.requests.length, 0);
    fake.finishRecording(upstreamId, 95);
    now.advance(config.talk.recordingTtlMs);
    const ready = (await ok(api('GET', path))).recording;
    assert.deepEqual([ready.status, ready.seconds, ready.price_tzs, ready.size_bytes, ready.ready_at !== null], ['ready', 95, 60, 1900000, true]);
    assert.match(ready.url, /^https:\/\/fake\.invalid\/.+\.mp4\?sig=/);
    for (let i = 0; i < 4; i += 1) assert.equal((await ok(api('GET', path))).recording.url, ready.url);
    assert.equal(fake.count('GET', `/recordings/${upstreamId}`), 1);
    // The link is good for an hour: a new one is fetched when it is 50 minutes old.
    now.advance(50 * 60 * 1000);
    const renewed = (await ok(api('GET', path))).recording;
    assert.notEqual(renewed.url, ready.url);
    assert.equal(fake.count('GET', `/recordings/${upstreamId}`), 2);

    assert.deepEqual((await ok(api('GET', '/v1/recordings'))).recordings.map((r) => r.id), [started.recording.id]);
    assert.deepEqual((await ok(api('GET', '/v1/recordings', { as: B }))).recordings, []);
    assert.deepEqual((await ok(api('GET', '/v1/recordings?room=desk', { as: B }))).recordings.map((r) => r.id), [started.recording.id]);
    assert.equal((await ok(api('GET', '/v1/account'))).spent_tzs, 60);

    // Once it is over the room can be recorded again.
    now.advance(1000);
    const next = await ok(api('POST', '/v1/recordings', { body: { room: 'desk' } }), 201);
    assert.notEqual(next.recording.id, started.recording.id);
    // Closing the room stops what is recording it.
    await ok(api('DELETE', '/v1/rooms/desk'));
    assert.deepEqual([...fake.recordings.values()].map((r) => r.status), ['ready', 'processing']);
  });

  scenario('recording a call: only while answered, stopped when the call ends', {}, async ({ api, fake }) => {
    const ids = await introduce(api);
    const { call } = await ok(api('POST', '/v1/calls', { body: { to: ids.b, video: true } }), 201);
    await fails(api('POST', '/v1/recordings', { body: { call: call.id } }), 409, 'conflict');
    await fails(api('POST', '/v1/recordings', { as: C, body: { call: call.id } }), 404, 'not_found');
    await ok(api('POST', `/v1/calls/${call.id}/accept`, { as: B }));

    const { recording } = await ok(api('POST', '/v1/recordings', { as: B, body: { call: call.id } }), 201);
    assert.deepEqual([recording.room, recording.call, recording.status], [null, call.id, 'recording']);
    // Upstream was given the call's room.
    assert.equal(fake.requests.at(-1).body.room, fake.appCalls.get(call.id).roomName);
    // Both people see the toggle on; either may see and stop the recording.
    assert.deepEqual((await ok(api('GET', `/v1/calls/${call.id}`))).call.recording, { id: recording.id, status: 'recording' });
    assert.equal((await ok(api('GET', `/v1/recordings/${recording.id}`))).recording.status, 'recording');
    assert.deepEqual((await ok(api('GET', `/v1/recordings?call=${call.id}`))).recordings.map((r) => r.id), [recording.id]);
    await fails(api('GET', `/v1/recordings/${recording.id}`, { as: C }), 404, 'not_found');

    // Hanging up stops it: nobody pays for a recording of an empty call.
    await ok(api('DELETE', `/v1/calls/${call.id}`));
    assert.deepEqual([...fake.recordings.values()].map((r) => r.status), ['processing']);
    assert.equal((await ok(api('GET', `/v1/recordings/${recording.id}`))).recording.status, 'processing');
    assert.equal((await ok(api('GET', `/v1/calls/${call.id}`))).call.recording, null);
  });

  scenario('recording a call upstream names no room for: refused, nothing started', {}, async ({ api, fake }) => {
    const ids = await introduce(api);
    fake.callsHaveRoom = false;
    const { call } = await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201);
    await ok(api('POST', `/v1/calls/${call.id}/accept`, { as: B }));
    await fails(api('POST', '/v1/recordings', { body: { call: call.id } }), 409, 'recording_unsupported');
    assert.equal(fake.count('POST', '/recordings'), 0);
  });

  scenario('recording: refusals start nothing; an unconfirmed start is found and can be stopped', {}, async ({ api, fake, now, app, config }) => {
    await introduce(api);
    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Desk' } }), 201);
    const start = () => api('POST', '/v1/recordings', { body: { room: 'desk' } });

    fake.balanceTzs = 0;
    await fails(start(), 402, 'balance_empty');
    assert.equal((await ok(api('GET', '/v1/account'))).balance.state, 'empty');
    fake.balanceTzs = 10000;
    fake.recordingConnected = false;
    await fails(start(), 503, 'not_connected');
    fake.recordingConnected = true;
    assert.deepEqual((await ok(api('GET', '/v1/recordings'))).recordings, []);
    assert.equal(fake.recordings.size, 0);

    // Upstream starts recording, then the answer is lost. The server looks
    // for the recording rather than leaving it running unseen.
    fake.fail('POST', '/recordings', 'drop');
    const found = await ok(start(), 201);
    assert.equal(found.recording.status, 'recording');
    assert.equal(fake.recordings.size, 1);
    assert.equal((await ok(api('GET', '/v1/account'))).balance.state, 'empty'); // nothing confirmed a charge
    assert.equal((await ok(api('DELETE', `/v1/recordings/${found.recording.id}`))).recording.status, 'processing');
    assert.equal([...fake.recordings.values()][0].status, 'processing');

    // No answer and nothing to be found: failed, and said so.
    now.advance(1000);
    fake.fail('POST', '/recordings', 'drop-before');
    const unsure = await fails(start(), 502, 'upstream_failed');
    assert.match(unsure.json.error.message, /may or may not/);
    assert.equal(fake.recordings.size, 1);
    assert.equal((await ok(api('GET', '/v1/recordings'))).recordings[0].status, 'failed');

    // A recording nobody stops is stopped by the server after an hour.
    now.advance(1000);
    const long = await ok(start(), 201);
    now.advance(config.recordings.maxMs - 1000);
    await app.routes.sweep();
    assert.equal((await ok(api('GET', '/v1/rooms/desk'))).room.recording?.id, long.recording.id);
    now.advance(1000);
    await app.routes.sweep();
    assert.equal((await ok(api('GET', '/v1/rooms/desk'))).room.recording, null);
    assert.deepEqual([...fake.recordings.values()].map((r) => r.status), ['processing', 'processing']);
  });

  scenario('activity: rooms, calls and phone calls in one list, newest first', { env: { PHONE_CALLS: 'on' } }, async ({ api, fake, now }) => {
    const ids = await introduce(api);
    assert.deepEqual((await ok(api('GET', '/v1/activity'))).items, []);

    await ok(api('POST', '/v1/rooms', { body: { display_name: 'Support desk' } }), 201);
    await ok(api('POST', '/v1/rooms', { as: C, body: { display_name: 'Not mine' } }), 201);
    now.advance(60000);
    const missed = (await ok(api('POST', '/v1/calls', { as: B, body: { to: ids.a, video: true } }), 201)).call;
    await ok(api('DELETE', `/v1/calls/${missed.id}`, { as: B }));
    now.advance(60000);
    const phone = (await ok(api('POST', '/v1/phone-calls', { body: { to: '+255712345678', request_id: 'dial-0001' } }), 201)).call;
    now.advance(60000);
    const talked = (await ok(api('POST', '/v1/calls', { body: { to: ids.b } }), 201)).call;
    await ok(api('POST', `/v1/calls/${talked.id}/accept`, { as: B }));
    now.advance(60000);
    await ok(api('POST', '/v1/rooms/support-desk/messages', { as: B, body: { text: 'Karibu   sana\nwote' } }), 201);

    const { items, now: stamp } = await ok(api('GET', '/v1/activity'));
    assert.equal(stamp, '2026-11-15T07:04:00Z');
    for (const item of items) {
      assert.deepEqual(Object.keys(item), ['kind', 'id', 'title', 'preview', 'at', 'unread', 'live', 'status', 'direction', 'video', 'seconds', 'peer']);
      assert.match(item.at, ISO);
    }
    assert.deepEqual(items, [
      {
        kind: 'room', id: 'support-desk', title: 'Support desk', preview: 'Baraka: Karibu sana wote', at: '2026-11-15T07:04:00Z',
        unread: 1, live: true, status: 'open', direction: null, video: null, seconds: null, peer: null,
      },
      {
        kind: 'call', id: talked.id, title: 'Baraka', preview: 'Voice call in progress', at: '2026-11-15T07:03:00Z',
        unread: 0, live: true, status: 'active', direction: 'outgoing', video: false, seconds: 60, peer: { identity: 'baraka', name: 'Baraka' },
      },
      {
        kind: 'phone', id: phone.id, title: '+255712345678', preview: 'Calling…', at: '2026-11-15T07:02:00Z',
        unread: 0, live: true, status: 'ringing', direction: 'outgoing', video: false, seconds: 0, peer: null,
      },
      {
        kind: 'call', id: missed.id, title: 'Baraka', preview: 'Missed video call', at: '2026-11-15T07:01:00Z',
        unread: 0, live: false, status: 'missed', direction: 'incoming', video: true, seconds: 0, peer: { identity: 'baraka', name: 'Baraka' },
      },
    ]);
    // The other side of the same things reads differently.
    const theirs = (await ok(api('GET', '/v1/activity', { as: B }))).items;
    assert.deepEqual(theirs.map((i) => [i.kind, i.preview]), [['room', 'You: Karibu sana wote'], ['call', 'Voice call in progress'], ['call', 'No answer']]);
    assert.deepEqual((await ok(api('GET', '/v1/activity?limit=2'))).items.map((i) => i.kind), ['room', 'call']);
    await fails(api('GET', '/v1/activity?limit=500'), 400, 'bad_request');

    // A finished call says how long it was; a room goes quiet after ten minutes.
    await ok(api('DELETE', `/v1/calls/${talked.id}`));
    fake.endPhone(fake.phoneCalls[0].id, 125);
    now.advance(11 * 60000);
    await ok(api('GET', `/v1/phone-calls/${phone.id}`));
    const later = (await ok(api('GET', '/v1/activity'))).items;
    assert.deepEqual(later.map((i) => [i.kind, i.preview, i.live]), [
      ['room', 'Baraka: Karibu sana wote', false],
      ['call', 'Voice call, 7 s', false],
      ['phone', 'Phone call, 2 min 5 s', false],
      ['call', 'Missed video call', false],
    ]);
  });
}
