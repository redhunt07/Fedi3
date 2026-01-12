#!/bin/sh
set -e

if [ "$(id -u)" = "0" ]; then
  if [ -d /data ]; then
    chown -R fedi3:fedi3 /data || true
  fi
  if ! gosu fedi3 sh -lc "touch /data/.relay_probe 2>/dev/null"; then
    echo "relay: /data is not writable by fedi3" >&2
  else
    gosu fedi3 sh -lc "rm -f /data/.relay_probe" || true
  fi
  exec gosu fedi3 /usr/local/bin/fedi3_relay
fi

exec /usr/local/bin/fedi3_relay
