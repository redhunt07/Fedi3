#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
# SPDX-License-Identifier: AGPL-3.0-only

import argparse
import json
import sys
import urllib.request
from urllib.parse import urlparse


def fetch_json(url, accept=None):
    headers = {}
    if accept:
        headers["Accept"] = accept
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = resp.read()
        return json.loads(body.decode("utf-8"))


def actor_username(actor_url):
    try:
        path = urlparse(actor_url).path.strip("/")
        if path.startswith("users/"):
            return path.split("/")[1]
        if path.startswith("@"):
            return path.split("/")[0][1:]
    except Exception:
        return ""
    return ""


def check_actor(actor):
    required = ["id", "type", "inbox", "outbox", "followers", "following", "publicKey"]
    missing = [k for k in required if k not in actor]
    return missing


def has_html_content(note_obj):
    content = (note_obj.get("content") or "").strip()
    return "<" in content and ">" in content


def has_source_plain(note_obj):
    src = note_obj.get("source") or {}
    if not isinstance(src, dict):
        return False
    media = (src.get("mediaType") or "").lower()
    return "text/plain" in media and bool((src.get("content") or "").strip())


def has_hashtag_tags(note_obj):
    tags = note_obj.get("tag") or []
    if not isinstance(tags, list):
        tags = [tags]
    for t in tags:
        if isinstance(t, dict) and t.get("type") == "Hashtag" and str(t.get("name", "")).startswith("#"):
            return True
    return False


def has_mention_tags(note_obj):
    tags = note_obj.get("tag") or []
    if not isinstance(tags, list):
        tags = [tags]
    for t in tags:
        if isinstance(t, dict) and t.get("type") == "Mention" and str(t.get("href", "")).startswith("http"):
            return True
    return False


def has_attachments(note_obj):
    att = note_obj.get("attachment") or []
    if not isinstance(att, list):
        att = [att]
    for a in att:
        if isinstance(a, dict):
            if a.get("url") and a.get("mediaType"):
                return True
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--actor", required=True, help="Actor URL, es. https://relay.fedi3.com/users/alice")
    ap.add_argument("--outbox-limit", type=int, default=20)
    ap.add_argument("--csv", action="store_true", help="Output CSV instead of JSON")
    args = ap.parse_args()

    actor_url = args.actor.strip()
    actor = fetch_json(
        actor_url,
        accept='application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", application/json',
    )

    missing = check_actor(actor)
    if missing:
        print("FAIL: actor missing fields:", ", ".join(missing))
        sys.exit(2)

    outbox_url = actor.get("outbox", "")
    outbox = fetch_json(outbox_url)
    first = outbox.get("first")
    if isinstance(first, dict):
        first = first.get("id") or first.get("href")
    page = fetch_json(first) if isinstance(first, str) else outbox
    items = page.get("orderedItems") or []
    if not items:
        print("WARN: outbox empty (cannot check notes)")
        items = []

    note_obj = None
    for it in items[: args.outbox_limit]:
        if not isinstance(it, dict):
            continue
        if it.get("type") in ("Create", "Update"):
            obj = it.get("object") or {}
            if isinstance(obj, dict) and obj.get("type") == "Note":
                note_obj = obj
                break
        if it.get("type") == "Note":
            note_obj = it
            break

    compat = {
        "mastodon": {
            "content_html": False,
            "hashtags": False,
            "mentions": False,
            "attachments": False,
        },
        "misskey_sharkey": {
            "featured": bool(actor.get("featured")),
            "featuredTags": bool(actor.get("featuredTags")),
            "content_html": False,
            "hashtags": False,
        },
        "pleroma_akkoma": {
            "content_html": False,
            "source_plain": False,
            "hashtags": False,
        },
        "pixelfed": {
            "content_html": False,
            "attachments": False,
        },
    }

    if note_obj:
        compat["mastodon"]["content_html"] = has_html_content(note_obj)
        compat["mastodon"]["hashtags"] = has_hashtag_tags(note_obj)
        compat["mastodon"]["mentions"] = has_mention_tags(note_obj)
        compat["mastodon"]["attachments"] = has_attachments(note_obj)

        compat["misskey_sharkey"]["content_html"] = has_html_content(note_obj)
        compat["misskey_sharkey"]["hashtags"] = has_hashtag_tags(note_obj)

        compat["pleroma_akkoma"]["content_html"] = has_html_content(note_obj)
        compat["pleroma_akkoma"]["source_plain"] = has_source_plain(note_obj)
        compat["pleroma_akkoma"]["hashtags"] = has_hashtag_tags(note_obj)

        compat["pixelfed"]["content_html"] = has_html_content(note_obj)
        compat["pixelfed"]["attachments"] = has_attachments(note_obj)

    if args.csv:
        rows = [
            "server,check,ok",
        ]
        for server, checks in compat.items():
            for check, ok in checks.items():
                rows.append(f"{server},{check},{str(bool(ok)).lower()}")
        print("\n".join(rows))
    else:
        print(json.dumps({"actor": actor_url, "checks": compat}, indent=2))
        if any(not all(v.values()) for v in compat.values()):
            print("WARN: some compatibility checks failed or are incomplete.")


if __name__ == "__main__":
    main()
