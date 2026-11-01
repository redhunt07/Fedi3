#!/bin/sh
set -e

if [ "$(id -u)" = "0" ]; then
  if [ -d /data ]; then
    chown -R "${P2P_UID:-1000}:${P2P_GID:-1000}" /data || true
  fi
  exec gosu appuser /usr/local/bin/fedi3_p2p_infra
fi

exec /usr/local/bin/fedi3_p2p_infra
