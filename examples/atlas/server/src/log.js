// Logging: one short line per event. Request lines carry method, path,
// status and duration; upstream lines carry the service, status and latency.
// Bodies, headers, audio and configuration values are never passed in, and
// known secrets are redacted in case an error message quotes one.

export function createLogger({ secrets = [], quiet = false, sink } = {}) {
  const write = sink ?? ((line) => process.stdout.write(`${line}\n`));

  function redact(text) {
    let out = String(text);
    for (const secret of secrets) out = out.split(secret).join('[redacted]');
    // Credentials inside any URL, whatever they are.
    out = out.replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+@/gi, '$1[redacted]@');
    return out.replace(/[\r\n]+/g, ' ').slice(0, 300);
  }

  function emit(level, message) {
    if (quiet) return;
    write(`${new Date().toISOString()} ${level} ${redact(message)}`);
  }

  return {
    redact,
    info: (message) => emit('info', message),
    warn: (message) => emit('warn', message),
    error: (message, err) => {
      const detail = err ? ` (${err.name ?? 'Error'}${err.code ? ` ${err.code}` : ''}: ${err.message ?? ''})` : '';
      emit('error', `${message}${detail}`);
    },
  };
}
