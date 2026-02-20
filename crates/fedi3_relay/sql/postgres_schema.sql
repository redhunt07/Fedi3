-- PostgreSQL schema for fedi3_relay.
-- This mirrors the SQLite schema in main.rs but uses Postgres types.

CREATE TABLE IF NOT EXISTS users (
  username TEXT PRIMARY KEY,
  token_sha256 TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  disabled BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_users_username_lower ON users (lower(username));

CREATE TABLE IF NOT EXISTS user_cache (
  username TEXT PRIMARY KEY,
  actor_json TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  actor_id TEXT NULL,
  actor_url TEXT NULL
);
ALTER TABLE user_cache ADD COLUMN IF NOT EXISTS actor_id TEXT;
ALTER TABLE user_cache ADD COLUMN IF NOT EXISTS actor_url TEXT;
CREATE INDEX IF NOT EXISTS idx_user_cache_updated ON user_cache(updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_user_cache_username_lower ON user_cache (lower(username));
CREATE INDEX IF NOT EXISTS idx_user_cache_actor_id_lower ON user_cache (lower(actor_id));
CREATE INDEX IF NOT EXISTS idx_user_cache_actor_url_lower ON user_cache (lower(actor_url));

CREATE TABLE IF NOT EXISTS user_collection_cache (
  username TEXT NOT NULL,
  kind TEXT NOT NULL,
  json TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  PRIMARY KEY(username, kind)
);

CREATE TABLE IF NOT EXISTS inbox_spool (
  id BIGSERIAL PRIMARY KEY,
  username TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  method TEXT NOT NULL,
  path TEXT NOT NULL,
  query TEXT NOT NULL,
  headers_json TEXT NOT NULL,
  body_b64 TEXT NOT NULL,
  body_len BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS inbox_spool_user_created ON inbox_spool(username, created_at_ms);

CREATE TABLE IF NOT EXISTS relay_registry (
  relay_url TEXT PRIMARY KEY,
  base_domain TEXT NULL,
  last_seen_ms BIGINT NOT NULL,
  last_telemetry_json TEXT NULL,
  sign_pubkey_b64 TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_relay_registry_seen ON relay_registry(last_seen_ms DESC);

CREATE TABLE IF NOT EXISTS relay_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS peer_registry (
  peer_id TEXT PRIMARY KEY,
  last_seen_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS peer_directory (
  peer_id TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  actor_url TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_peer_directory_user ON peer_directory(username);
CREATE INDEX IF NOT EXISTS idx_peer_directory_actor ON peer_directory(actor_url);
CREATE INDEX IF NOT EXISTS idx_peer_directory_user_lower ON peer_directory (lower(username));
CREATE INDEX IF NOT EXISTS idx_peer_directory_actor_lower ON peer_directory (lower(actor_url));
CREATE INDEX IF NOT EXISTS idx_peer_directory_updated ON peer_directory(updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS relay_user_directory (
  actor_url TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  relay_url TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name ON relay_user_directory(username);
CREATE INDEX IF NOT EXISTS idx_relay_user_dir_relay ON relay_user_directory(relay_url);
CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name_lower ON relay_user_directory (lower(username));
CREATE INDEX IF NOT EXISTS idx_relay_user_dir_actor_lower ON relay_user_directory (lower(actor_url));
CREATE INDEX IF NOT EXISTS idx_relay_user_dir_updated ON relay_user_directory(updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS user_moves (
  username TEXT PRIMARY KEY,
  moved_to_actor TEXT NOT NULL,
  moved_at_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS move_notices (
  notice_id TEXT PRIMARY KEY,
  notice_json TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_move_notices_created ON move_notices(created_at_ms DESC);

CREATE TABLE IF NOT EXISTS move_notice_fanout (
  notice_id TEXT NOT NULL,
  relay_url TEXT NOT NULL,
  tries BIGINT NOT NULL,
  last_try_ms BIGINT NOT NULL,
  sent_ok BOOLEAN NOT NULL,
  PRIMARY KEY(notice_id, relay_url)
);

CREATE TABLE IF NOT EXISTS media_items (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  backend TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  media_type TEXT NOT NULL,
  size BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_user_created ON media_items(username, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS user_backups (
  username TEXT PRIMARY KEY,
  storage_key TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL,
  meta_json TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_backups_updated ON user_backups(updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS user_backups_history (
  storage_key TEXT PRIMARY KEY,
  username TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  meta_json TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_backups_hist_user_created ON user_backups_history(username, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS relay_notes (
  note_id TEXT PRIMARY KEY,
  actor_id TEXT NULL,
  published_ms BIGINT NULL,
  content_text TEXT NOT NULL,
  content_html TEXT NOT NULL,
  note_json TEXT NOT NULL,
  created_at_ms BIGINT NOT NULL,
  search_tsv tsvector GENERATED ALWAYS AS (
    to_tsvector('simple', coalesce(content_text, '') || ' ' || coalesce(content_html, ''))
  ) STORED
);
CREATE INDEX IF NOT EXISTS idx_relay_notes_created ON relay_notes(created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_relay_notes_actor ON relay_notes(actor_id);
CREATE INDEX IF NOT EXISTS idx_relay_notes_published ON relay_notes(published_ms DESC);
CREATE INDEX IF NOT EXISTS idx_relay_notes_search_tsv ON relay_notes USING GIN (search_tsv);

CREATE TABLE IF NOT EXISTS relay_note_tags (
  note_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  tag_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', coalesce(tag, ''))) STORED,
  PRIMARY KEY(note_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag ON relay_note_tags(tag);
CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag_lower ON relay_note_tags (lower(tag));
CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tsv ON relay_note_tags USING GIN (tag_tsv);

CREATE TABLE IF NOT EXISTS relay_tag_counts (
  tag TEXT PRIMARY KEY,
  count BIGINT NOT NULL
);
INSERT INTO relay_tag_counts(tag, count)
SELECT tag, COUNT(*) FROM relay_note_tags GROUP BY tag
ON CONFLICT (tag) DO UPDATE SET count = EXCLUDED.count;
DELETE FROM relay_tag_counts WHERE tag NOT IN (SELECT tag FROM relay_note_tags);

CREATE TABLE IF NOT EXISTS relay_notes_count (
  id SMALLINT PRIMARY KEY,
  count BIGINT NOT NULL
);
INSERT INTO relay_notes_count(id, count)
SELECT 1, COUNT(*) FROM relay_notes
ON CONFLICT (id) DO UPDATE SET count = EXCLUDED.count;

CREATE OR REPLACE FUNCTION relay_tag_counts_insert() RETURNS trigger AS $$
BEGIN
  INSERT INTO relay_tag_counts(tag, count) VALUES (NEW.tag, 1)
  ON CONFLICT (tag) DO UPDATE SET count = relay_tag_counts.count + 1;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION relay_tag_counts_delete() RETURNS trigger AS $$
BEGIN
  UPDATE relay_tag_counts SET count = count - 1 WHERE tag = OLD.tag;
  DELETE FROM relay_tag_counts WHERE tag = OLD.tag AND count <= 0;
  RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'relay_tag_counts_insert_tr') THEN
    CREATE TRIGGER relay_tag_counts_insert_tr
      AFTER INSERT ON relay_note_tags
      FOR EACH ROW EXECUTE FUNCTION relay_tag_counts_insert();
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'relay_tag_counts_delete_tr') THEN
    CREATE TRIGGER relay_tag_counts_delete_tr
      AFTER DELETE ON relay_note_tags
      FOR EACH ROW EXECUTE FUNCTION relay_tag_counts_delete();
  END IF;
END;
$$;

CREATE OR REPLACE FUNCTION relay_notes_count_insert() RETURNS trigger AS $$
BEGIN
  UPDATE relay_notes_count SET count = count + 1 WHERE id = 1;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION relay_notes_count_delete() RETURNS trigger AS $$
BEGIN
  UPDATE relay_notes_count SET count = count - 1 WHERE id = 1;
  RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'relay_notes_count_insert_tr') THEN
    CREATE TRIGGER relay_notes_count_insert_tr
      AFTER INSERT ON relay_notes
      FOR EACH ROW EXECUTE FUNCTION relay_notes_count_insert();
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'relay_notes_count_delete_tr') THEN
    CREATE TRIGGER relay_notes_count_delete_tr
      AFTER DELETE ON relay_notes
      FOR EACH ROW EXECUTE FUNCTION relay_notes_count_delete();
  END IF;
END;
$$;

ALTER TABLE relay_notes
  ADD COLUMN IF NOT EXISTS search_tsv tsvector GENERATED ALWAYS AS (
    to_tsvector('simple', coalesce(content_text, '') || ' ' || coalesce(content_html, ''))
  ) STORED;
ALTER TABLE relay_note_tags
  ADD COLUMN IF NOT EXISTS tag_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', coalesce(tag, ''))) STORED;

CREATE TABLE IF NOT EXISTS relay_media (
  media_url TEXT PRIMARY KEY,
  media_type TEXT NULL,
  name TEXT NULL,
  width BIGINT NULL,
  height BIGINT NULL,
  blurhash TEXT NULL,
  created_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_relay_media_created ON relay_media(created_at_ms DESC);

CREATE TABLE IF NOT EXISTS relay_actors (
  actor_url TEXT PRIMARY KEY,
  username TEXT NULL,
  actor_json TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_relay_actors_updated ON relay_actors(updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_relay_actors_username_lower ON relay_actors (lower(username));
CREATE INDEX IF NOT EXISTS idx_relay_actors_url_lower ON relay_actors (lower(actor_url));

CREATE TABLE IF NOT EXISTS relay_reputation (
  relay_url TEXT PRIMARY KEY,
  score INTEGER NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_relay_reputation_updated ON relay_reputation(updated_at_ms DESC);

CREATE TABLE IF NOT EXISTS relay_outbox_index (
  username TEXT PRIMARY KEY,
  last_index_ms BIGINT NOT NULL,
  last_ok BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_audit (
  id BIGSERIAL PRIMARY KEY,
  action TEXT NOT NULL,
  username TEXT NULL,
  actor TEXT NULL,
  ip TEXT NULL,
  ok BOOLEAN NOT NULL,
  detail TEXT NULL,
  created_at_ms BIGINT NOT NULL,
  request_id TEXT NULL,
  correlation_id TEXT NULL,
  user_agent TEXT NULL
);
CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit(created_at_ms DESC);
ALTER TABLE admin_audit ADD COLUMN IF NOT EXISTS request_id TEXT;
ALTER TABLE admin_audit ADD COLUMN IF NOT EXISTS correlation_id TEXT;
ALTER TABLE admin_audit ADD COLUMN IF NOT EXISTS user_agent TEXT;
