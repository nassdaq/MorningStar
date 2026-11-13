// In-memory state: spoken answers waiting to be fetched, the per-device rate
// limit, and the per-chat lock. All of it is lost on restart, which is
// acceptable for each: audio is re-playable only for ten minutes anyway, a
// rate window is a minute long, and a lock only matters while a request runs.
// (The same three stores as the CallReady server's.)

import { randomBytes } from 'node:crypto';

/**
 * Spoken answers by id, each owned by the device whose message produced it.
 * Entries live for `ttlMs`; when the store is over `maxBytes` the oldest go
 * first.
 */
export function createAudioStore({ ttlMs, maxBytes, now = Date.now }) {
  const entries = new Map(); // insertion order is age order
  let bytes = 0;

  function drop(id) {
    const entry = entries.get(id);
    if (!entry) return;
    bytes -= entry.bytes.length;
    entries.delete(id);
  }

  function sweep() {
    const time = now();
    for (const [id, entry] of entries) {
      if (entry.expires > time) break;
      drop(id);
    }
  }

  return {
    /** Stores a clip and returns its id, or null when it cannot be kept. */
    put(deviceId, clip) {
      sweep();
      if (clip.length > maxBytes) return null;
      while (bytes + clip.length > maxBytes && entries.size > 0) drop(entries.keys().next().value);
      const id = `a_${randomBytes(12).toString('hex')}`;
      entries.set(id, { deviceId, bytes: clip, expires: now() + ttlMs });
      bytes += clip.length;
      return id;
    },

    /** The clip, if it exists, has not expired and belongs to the device. */
    get(deviceId, id) {
      const entry = entries.get(id);
      if (!entry) return null;
      if (entry.expires <= now()) {
        drop(id);
        return null;
      }
      return entry.deviceId === deviceId ? entry.bytes : null;
    },

    sweep,
    stats: () => ({ clips: entries.size, bytes }),
  };
}

/** A sliding window of `limit` requests per `windowMs` for each key. */
export function createRateLimiter({ limit, windowMs = 60000, now = Date.now }) {
  const hits = new Map();

  return {
    /** Records a request. Returns 0 when allowed, else seconds to wait. */
    take(key) {
      const time = now();
      const recent = (hits.get(key) ?? []).filter((t) => t > time - windowMs);
      if (recent.length >= limit) {
        hits.set(key, recent);
        return Math.max(1, Math.ceil((recent[0] + windowMs - time) / 1000));
      }
      recent.push(time);
      hits.set(key, recent);
      return 0;
    },

    sweep() {
      const time = now();
      for (const [key, list] of hits) {
        if (list.length === 0 || list[list.length - 1] <= time - windowMs) hits.delete(key);
      }
    },
  };
}

/**
 * Runs the work for one key one at a time, in arrival order: two messages
 * for the same chat cannot interleave.
 */
export function createKeyedLock() {
  const tails = new Map();

  return async function withLock(key, work) {
    const previous = tails.get(key) ?? Promise.resolve();
    let release;
    const mine = new Promise((resolve) => {
      release = resolve;
    });
    const tail = previous.then(() => mine);
    tails.set(key, tail);
    await previous;
    try {
      return await work();
    } finally {
      release();
      if (tails.get(key) === tail) tails.delete(key);
    }
  };
}
