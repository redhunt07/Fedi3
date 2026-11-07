#!/usr/bin/env python3
"""Validate the shared Fedi3 relay directory (relays/relays.json)."""
from __future__ import annotations

import json
import re
import sys
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / "relays" / "relays.json"
SCHEMA = "fedi3-relay-directory/v1"
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


class ValidationError(Exception):
    pass


def _is_loopback(host: str) -> bool:
    return host in {"localhost", "127.0.0.1", "::1", "[::1]"}


def _check_base_url(value: object, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValidationError(f"{field} must be a non-empty string")
    value = value.strip()
    if value != value.rstrip("/"):
        raise ValidationError(f"{field} must not end with a slash: {value}")
    parsed = urlparse(value)
    if parsed.scheme not in {"https", "http"}:
        raise ValidationError(f"{field} must use https://: {value}")
    if parsed.scheme == "http" and not _is_loopback(parsed.hostname or ""):
        raise ValidationError(f"{field} must use https:// outside loopback: {value}")
    if not parsed.hostname:
        raise ValidationError(f"{field} has no hostname: {value}")
    if parsed.query or parsed.fragment:
        raise ValidationError(f"{field} must not contain a query or fragment: {value}")
    return value


def _check_ws(value: object, base: str, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValidationError(f"{field} must be a non-empty string")
    value = value.strip()
    if value != value.rstrip("/"):
        raise ValidationError(f"{field} must not end with a slash: {value}")
    parsed = urlparse(value)
    if parsed.scheme not in {"wss", "ws"}:
        raise ValidationError(f"{field} must use wss://: {value}")
    if parsed.scheme == "ws" and not _is_loopback(parsed.hostname or ""):
        raise ValidationError(f"{field} must use wss:// outside loopback: {value}")
    base_host = urlparse(base).hostname
    if parsed.hostname != base_host:
        raise ValidationError(f"{field} must use the same host as url: {value}")
    return value


def _check_date(value: object, field: str) -> str:
    if not isinstance(value, str) or not DATE_RE.match(value):
        raise ValidationError(f"{field} must be an ISO date (YYYY-MM-DD)")
    try:
        datetime.strptime(value, "%Y-%m-%d")
    except ValueError as exc:
        raise ValidationError(f"{field} is not a valid date: {value}") from exc
    return value


def validate(payload: object) -> dict:
    if not isinstance(payload, dict):
        raise ValidationError("directory root must be a JSON object")
    if payload.get("schema") != SCHEMA:
        raise ValidationError(f"schema must be {SCHEMA!r}")
    updated_at = payload.get("updated_at")
    if not isinstance(updated_at, str) or not updated_at.strip():
        raise ValidationError("updated_at must be an ISO 8601 timestamp")
    try:
        datetime.fromisoformat(updated_at.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValidationError(f"updated_at is not a valid timestamp: {updated_at}") from exc

    relays = payload.get("relays")
    if not isinstance(relays, list) or not relays:
        raise ValidationError("relays must be a non-empty array")

    seen_urls: set[str] = set()
    seen_hosts: set[str] = set()
    seen_ws: set[str] = set()
    previous_added: str | None = None

    for index, entry in enumerate(relays):
        where = f"relays[{index}]"
        if not isinstance(entry, dict):
            raise ValidationError(f"{where} must be an object")
        base = _check_base_url(entry.get("url"), f"{where}.url")
        host = urlparse(base).hostname
        if base in seen_urls:
            raise ValidationError(f"duplicate relay url: {base}")
        if host in seen_hosts:
            raise ValidationError(f"duplicate relay host: {host}")
        seen_urls.add(base)
        seen_hosts.add(host)

        ws = entry.get("ws")
        if ws is not None:
            ws_value = _check_ws(ws, base, f"{where}.ws")
            if ws_value in seen_ws:
                raise ValidationError(f"duplicate relay ws: {ws_value}")
            seen_ws.add(ws_value)

        for field in ("name", "operator", "contact"):
            value = entry.get(field)
            if not isinstance(value, str) or not value.strip():
                raise ValidationError(f"{where}.{field} must be a non-empty string")

        added = _check_date(entry.get("added"), f"{where}.added")
        if previous_added is not None and added < previous_added:
            raise ValidationError(
                f"relays must be ordered by added date ({where}.added is earlier than the previous entry)"
            )
        previous_added = added

        region = entry.get("region")
        if region is not None and (not isinstance(region, str) or not region.strip()):
            raise ValidationError(f"{where}.region must be a non-empty string when present")

    return payload


def main() -> int:
    try:
        payload = json.loads(DIRECTORY.read_text(encoding="utf-8"))
        validate(payload)
        bundled = ROOT / "app" / "assets" / "relays.json"
        if not bundled.exists():
            raise ValidationError("app/assets/relays.json is missing")
        bundled_payload = json.loads(bundled.read_text(encoding="utf-8"))
        if bundled_payload != payload:
            raise ValidationError(
                "app/assets/relays.json must match relays/relays.json "
                "(copy the shared list into the app bundle)"
            )
    except (OSError, json.JSONDecodeError, ValidationError) as exc:
        print(f"relay directory invalid: {exc}", file=sys.stderr)
        return 1
    print(f"relay directory OK ({len(payload['relays'])} relays)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
