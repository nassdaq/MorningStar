// What the server keeps in memory: the per-device rate limit and a way to
// share one piece of work between everyone who asks for it at once.

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
 * Coalesces work by key: while a piece of work for a key is running,
 * everyone else who asks for it waits for that same run instead of
 * starting another. Ten phones polling one call cause one upstream request.
 */
export function createSharedWork() {
  const running = new Map();

  return function shared(key, work) {
    const existing = running.get(key);
    if (existing) return existing;
    const mine = (async () => {
      try {
        return await work();
      } finally {
        running.delete(key);
      }
    })();
    running.set(key, mine);
    return mine;
  };
}
