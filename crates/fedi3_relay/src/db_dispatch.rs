//! Bounded blocking SQL dispatch; PostgreSQL connections come from its shared pool.
use super::*;
#[derive(Clone)]
pub(super) struct DbDispatch {
    inner: Arc<Db>,
    permits: Arc<Semaphore>,
    probe: Arc<Semaphore>,
}
impl DbDispatch {
    pub(super) fn new(db: Db) -> Self {
        let concurrency = if db.driver == DbDriver::Postgres {
            db.pg_pool_max_size.saturating_sub(1).max(1)
        } else {
            16
        };
        Self {
            inner: Arc::new(db),
            permits: Arc::new(Semaphore::new(concurrency)),
            probe: Arc::new(Semaphore::new(1)),
        }
    }
    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Db) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit =
            tokio::time::timeout(Duration::from_secs(5), self.permits.clone().acquire_owned())
                .await??;
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(&db)
        })
        .await?
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn append_forward_receipt(
        &self,
        user: String,
        relay: String,
        event: crate::receipts::ReceiptEvent,
        public: String,
        secret: [u8; 32],
        ttl: u64,
        max: u64,
    ) -> Result<String> {
        self.run(move |db| {
            db.append_forward_receipt(&user, &relay, event, &public, &secret, ttl, max)
        })
        .await
    }
    pub(super) async fn forward_receipt_rows(
        &self,
        user: Option<String>,
        after: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>> {
        self.run(move |db| db.forward_receipt_rows(user.as_deref(), after, limit))
            .await
    }
    pub(super) async fn prune_forward_receipts(&self, ttl: u64) -> Result<()> {
        self.run(move |db| db.prune_forward_receipts(ttl)).await
    }
    pub(super) async fn pending_push_users(
        &self,
        after: String,
        cutoff: i64,
    ) -> Result<Vec<String>> {
        self.run(move |db| {
            let sql="SELECT DISTINCT s.username FROM inbox_spool s JOIN users u ON u.username=s.username WHERE s.username>?1 AND s.created_at_ms>=?2 AND u.disabled=0 ORDER BY s.username LIMIT 64";
            match db.driver {
                DbDriver::Sqlite=>{
                    let conn=db.open_sqlite_conn()?;let mut stmt=conn.prepare(sql)?;
                    let rows=stmt.query_map(params![after,cutoff],|r|r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
                    Ok(rows)
                }
                DbDriver::Postgres=>{
                    let mut conn=db.open_pg_conn()?;
                    Ok(conn.query(&sql.replace("?1","$1").replace("?2","$2").replace("disabled=0","disabled=false"),&[&after,&cutoff])?.iter().map(|r|r.get(0)).collect())
                }
            }
        }).await
    }
    pub(super) async fn health_check(&self) -> Result<()> {
        let permit = self.probe.clone().acquire_owned().await?;
        let db = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            db.health_check()
        })
        .await?
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn insert_admin_audit(
        &self,
        action: &str,
        username: Option<&str>,
        actor: Option<&str>,
        ip: Option<&str>,
        ok: bool,
        detail: Option<&str>,
        meta: &AuditMeta,
    ) -> Result<()> {
        let action = action.to_owned();
        let username = username.map(str::to_owned);
        let actor = actor.map(str::to_owned);
        let ip = ip.map(str::to_owned);
        let detail = detail.map(str::to_owned);
        let meta = meta.clone();
        self.run(move |db| {
            db.insert_admin_audit(
                &action,
                username.as_deref(),
                actor.as_deref(),
                ip.as_deref(),
                ok,
                detail.as_deref(),
                &meta,
            )
        })
        .await
    }
    pub(super) async fn list_admin_audit(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<
        Vec<(
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
            Option<String>,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        self.run(move |db| db.list_admin_audit(limit, offset)).await
    }
    pub(super) async fn upsert_relay(
        &self,
        relay_url: &str,
        base_domain: Option<String>,
        telemetry_json: Option<String>,
        sign_pubkey_b64: Option<String>,
    ) -> Result<()> {
        let relay_url = relay_url.to_owned();
        self.run(move |db| {
            db.upsert_relay(&relay_url, base_domain, telemetry_json, sign_pubkey_b64)
        })
        .await
    }
    pub(super) async fn list_relays(
        &self,
        limit: u32,
    ) -> Result<Vec<(String, Option<String>, i64, Option<String>, Option<String>)>> {
        self.run(move |db| db.list_relays(limit)).await
    }
    pub(super) async fn get_relay_pubkey_b64(&self, relay_url: &str) -> Result<Option<String>> {
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.get_relay_pubkey_b64(&relay_url))
            .await
    }
    pub(super) async fn relay_meta_get(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        self.run(move |db| db.relay_meta_get(&key)).await
    }
    pub(super) async fn relay_meta_set(&self, key: &str, value: &str) -> Result<()> {
        let key = key.to_owned();
        let value = value.to_owned();
        self.run(move |db| db.relay_meta_set(&key, &value)).await
    }
    pub(super) async fn list_relay_sync_state(&self) -> Result<Vec<(String, i64)>> {
        self.run(move |db| db.list_relay_sync_state()).await
    }
    pub(super) async fn load_or_create_signing_keypair_b64(&self) -> Result<(String, String)> {
        self.run(move |db| db.load_or_create_signing_keypair_b64())
            .await
    }
    pub(super) async fn upsert_peer_seen(&self, peer_id: &str) -> Result<()> {
        let peer_id = peer_id.to_owned();
        self.run(move |db| db.upsert_peer_seen(&peer_id)).await
    }
    pub(super) async fn count_peers_seen_since(&self, cutoff_ms: i64) -> Result<u64> {
        self.run(move |db| db.count_peers_seen_since(cutoff_ms))
            .await
    }
    pub(super) async fn count_users_total(&self) -> Result<u64> {
        self.run(move |db| db.count_users_total()).await
    }
    #[cfg(test)]
    pub(super) async fn create_user(&self, username: &str, token: &str) -> Result<bool> {
        let username = username.to_owned();
        let token = token.to_owned();
        self.run(move |db| db.create_user(&username, &token)).await
    }

    pub(super) async fn verify_user_token(&self, username: &str, token: &str) -> Result<bool> {
        let username = username.to_owned();
        let token = token.to_owned();
        self.run(move |db| db.verify_user_token(&username, &token))
            .await
    }
    pub(super) async fn user_exists(&self, username: &str) -> Result<bool> {
        let username = username.to_owned();
        self.run(move |db| db.user_exists(&username)).await
    }
    pub(super) async fn upsert_user_token(
        &self,
        cfg: &RelayConfig,
        headers: &HeaderMap,
        username: &str,
        new_token: &str,
    ) -> Result<UpsertUserResult> {
        let cfg = cfg.clone();
        let headers = headers.clone();
        let username = username.to_owned();
        let new_token = new_token.to_owned();
        self.run(move |db| db.upsert_user_token(&cfg, &headers, &username, &new_token))
            .await
    }
    pub(super) async fn verify_or_register(
        &self,
        cfg: &RelayConfig,
        username: &str,
        token: &str,
    ) -> Result<()> {
        let cfg = cfg.clone();
        let username = username.to_owned();
        let token = token.to_owned();
        self.run(move |db| db.verify_or_register(&cfg, &username, &token))
            .await
    }
    pub(super) async fn verify_token(&self, username: &str, token: &str) -> Result<bool> {
        let username = username.to_owned();
        let token = token.to_owned();
        self.run(move |db| db.verify_token(&username, &token)).await
    }
    pub(super) async fn upsert_user_identity(&self, username: &str, envelope: &str) -> Result<()> {
        let username = username.to_owned();
        let envelope = envelope.to_owned();
        self.run(move |db| db.upsert_user_identity(&username, &envelope))
            .await
    }
    pub(super) async fn get_user_identity(&self, username: &str) -> Result<Option<String>> {
        let username = username.to_owned();
        self.run(move |db| db.get_user_identity(&username)).await
    }
    pub(super) async fn put_pair_session(
        &self,
        code: &str,
        username: &str,
        envelope: &str,
        expires_at_ms: i64,
    ) -> Result<()> {
        let code = code.to_owned();
        let username = username.to_owned();
        let envelope = envelope.to_owned();
        self.run(move |db| db.put_pair_session(&code, &username, &envelope, expires_at_ms))
            .await
    }
    pub(super) async fn get_pair_session(&self, code: &str, now: i64) -> Result<Option<String>> {
        let code = code.to_owned();
        self.run(move |db| db.get_pair_session(&code, now)).await
    }
    pub(super) async fn prune_pair_sessions(&self, now: i64) -> Result<()> {
        self.run(move |db| db.prune_pair_sessions(now)).await
    }
    pub(super) async fn insert_chat_spool(&self, username: &str, envelope: &str) -> Result<i64> {
        let username = username.to_owned();
        let envelope = envelope.to_owned();
        self.run(move |db| db.insert_chat_spool(&username, &envelope))
            .await
    }
    pub(super) async fn chat_spool_after(
        &self,
        username: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String)>> {
        let username = username.to_owned();
        self.run(move |db| db.chat_spool_after(&username, after_id, limit))
            .await
    }
    pub(super) async fn prune_chat_spool(&self, cutoff_ms: i64) -> Result<()> {
        self.run(move |db| db.prune_chat_spool(cutoff_ms)).await
    }
    pub(super) async fn trim_chat_spool(&self, username: &str, keep: i64) -> Result<()> {
        let username = username.to_owned();
        self.run(move |db| db.trim_chat_spool(&username, keep))
            .await
    }
    pub(super) async fn set_user_move(&self, username: &str, moved_to_actor: &str) -> Result<()> {
        let username = username.to_owned();
        let moved_to_actor = moved_to_actor.to_owned();
        self.run(move |db| db.set_user_move(&username, &moved_to_actor))
            .await
    }
    pub(super) async fn clear_user_move(&self, username: &str) -> Result<()> {
        let username = username.to_owned();
        self.run(move |db| db.clear_user_move(&username)).await
    }
    pub(super) async fn get_user_move(&self, username: &str) -> Result<Option<(String, i64)>> {
        let username = username.to_owned();
        self.run(move |db| db.get_user_move(&username)).await
    }
    pub(super) async fn has_move_notice(&self, notice_id: &str) -> Result<bool> {
        let notice_id = notice_id.to_owned();
        self.run(move |db| db.has_move_notice(&notice_id)).await
    }
    pub(super) async fn upsert_move_notice(
        &self,
        notice_id: &str,
        notice_json: &str,
    ) -> Result<()> {
        let notice_id = notice_id.to_owned();
        let notice_json = notice_json.to_owned();
        self.run(move |db| db.upsert_move_notice(&notice_id, &notice_json))
            .await
    }
    pub(super) async fn list_recent_move_notices(
        &self,
        cutoff_ms: i64,
        limit: u32,
    ) -> Result<Vec<(String, String, i64)>> {
        self.run(move |db| db.list_recent_move_notices(cutoff_ms, limit))
            .await
    }
    pub(super) async fn get_fanout_status(
        &self,
        notice_id: &str,
        relay_url: &str,
    ) -> Result<Option<(i64, i64, i64)>> {
        let notice_id = notice_id.to_owned();
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.get_fanout_status(&notice_id, &relay_url))
            .await
    }
    pub(super) async fn record_fanout_attempt(
        &self,
        notice_id: &str,
        relay_url: &str,
        sent_ok: bool,
    ) -> Result<()> {
        let notice_id = notice_id.to_owned();
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.record_fanout_attempt(&notice_id, &relay_url, sent_ok))
            .await
    }
    pub(super) async fn count_users(&self) -> Result<u64> {
        self.run(move |db| db.count_users()).await
    }
    pub(super) async fn list_users(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<(String, i64, i64)>> {
        self.run(move |db| db.list_users(limit, offset)).await
    }
    pub(super) async fn set_disabled(&self, username: &str, disabled: bool) -> Result<()> {
        let username = username.to_owned();
        self.run(move |db| db.set_disabled(&username, disabled))
            .await
    }
    pub(super) async fn rotate_token(&self, username: &str, new_token: &str) -> Result<()> {
        let username = username.to_owned();
        let new_token = new_token.to_owned();
        self.run(move |db| db.rotate_token(&username, &new_token))
            .await
    }
    pub(super) async fn get_user(&self, username: &str) -> Result<Option<(i64, i64)>> {
        let username = username.to_owned();
        self.run(move |db| db.get_user(&username)).await
    }
    pub(super) async fn list_user_media(&self, username: &str) -> Result<Vec<MediaItem>> {
        let username = username.to_owned();
        self.run(move |db| db.list_user_media(&username)).await
    }
    pub(super) async fn expired_media_reservations(
        &self,
        backend: &str,
        cutoff: i64,
    ) -> Result<Vec<MediaItem>> {
        let backend = backend.to_owned();
        self.run(move |db| db.expired_media_reservations(&backend, cutoff))
            .await
    }
    pub(super) async fn reserve_media_item(
        &self,
        item: &MediaItem,
        quota: u64,
    ) -> Result<MediaReservation> {
        let item = item.clone();
        self.run(move |db| db.reserve_media_item(&item, quota))
            .await
    }
    pub(super) async fn finish_media_upload(&self, item: &MediaItem) -> Result<bool> {
        let item = item.clone();
        self.run(move |db| db.finish_media_upload(&item)).await
    }
    pub(super) async fn delete_user(&self, username: &str) -> Result<bool> {
        let username = username.to_owned();
        self.run(move |db| db.delete_user(&username)).await
    }
    pub(super) async fn is_user_enabled(&self, username: &str) -> Result<bool> {
        let username = username.to_owned();
        self.run(move |db| db.is_user_enabled(&username)).await
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn enqueue_spool(
        &self,
        cfg: &RelayConfig,
        username: &str,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body_b64: &str,
        body_len: i64,
    ) -> Result<()> {
        let cfg = cfg.clone();
        let username = username.to_owned();
        let method = method.to_owned();
        let path = path.to_owned();
        let query = query.to_owned();
        let headers = headers.to_vec();
        let body_b64 = body_b64.to_owned();
        self.run(move |db| {
            db.enqueue_spool(
                &cfg, &username, &method, &path, &query, &headers, &body_b64, body_len,
            )
        })
        .await
    }
    pub(super) async fn list_spool(&self, username: &str, limit: usize) -> Result<Vec<SpoolItem>> {
        let username = username.to_owned();
        self.run(move |db| db.list_spool(&username, limit)).await
    }
    pub(super) async fn delete_spool_ids(&self, ids: &[i64]) -> Result<()> {
        let ids = ids.to_vec();
        self.run(move |db| db.delete_spool_ids(&ids)).await
    }
    pub(super) async fn cleanup_spool(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_spool(ttl_secs)).await
    }
    pub(super) async fn cleanup_move_notices(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_move_notices(ttl_secs)).await
    }
    pub(super) async fn spool_stats(&self, username: &str) -> Result<(u64, u64)> {
        let username = username.to_owned();
        self.run(move |db| db.spool_stats(&username)).await
    }
    pub(super) async fn upsert_peer(&self, relay_url: &str) -> Result<()> {
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.upsert_peer(&relay_url)).await
    }
    pub(super) async fn list_peers(&self) -> Result<Vec<(String, Option<i64>)>> {
        self.run(move |db| db.list_peers()).await
    }
    pub(super) async fn mark_peer_seen(&self, relay_url: &str) -> Result<()> {
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.mark_peer_seen(&relay_url)).await
    }
    pub(super) async fn delete_peer(&self, relay_url: &str) -> Result<()> {
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.delete_peer(&relay_url)).await
    }
    pub(super) async fn upsert_remote_user(&self, username: &str, relay_url: &str) -> Result<()> {
        let username = username.to_owned();
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.upsert_remote_user(&username, &relay_url))
            .await
    }
    pub(super) async fn get_remote_user_relay(&self, username: &str) -> Result<Option<String>> {
        let username = username.to_owned();
        self.run(move |db| db.get_remote_user_relay(&username))
            .await
    }
    pub(super) async fn prune_remote_users(&self, older_than_ms: i64) -> Result<()> {
        self.run(move |db| db.prune_remote_users(older_than_ms))
            .await
    }
    pub(super) async fn list_local_usernames(&self) -> Result<Vec<String>> {
        self.run(move |db| db.list_local_usernames()).await
    }
    pub(super) async fn user_token_matches(&self, username: &str, token: &str) -> Result<bool> {
        let username = username.to_owned();
        let token = token.to_owned();
        self.run(move |db| db.user_token_matches(&username, &token))
            .await
    }
    pub(super) async fn upsert_relay_block(
        &self,
        kind: &str,
        value: &str,
        reason: Option<&str>,
        created_by: Option<&str>,
        expires_at_ms: Option<i64>,
    ) -> Result<()> {
        let kind = kind.to_owned();
        let value = value.to_owned();
        let reason = reason.map(str::to_owned);
        let created_by = created_by.map(str::to_owned);
        self.run(move |db| {
            db.upsert_relay_block(
                &kind,
                &value,
                reason.as_deref(),
                created_by.as_deref(),
                expires_at_ms,
            )
        })
        .await
    }
    pub(super) async fn delete_relay_block(&self, kind: &str, value: &str) -> Result<()> {
        let kind = kind.to_owned();
        let value = value.to_owned();
        self.run(move |db| db.delete_relay_block(&kind, &value))
            .await
    }
    pub(super) async fn list_relay_blocks(&self) -> Result<Vec<RelayBlockRow>> {
        self.run(move |db| db.list_relay_blocks()).await
    }
    pub(super) async fn replace_user_blocks(
        &self,
        username: &str,
        actors: &[String],
        domains: &[String],
    ) -> Result<()> {
        let username = username.to_owned();
        let actors = actors.to_vec();
        let domains = domains.to_vec();
        self.run(move |db| db.replace_user_blocks(&username, &actors, &domains))
            .await
    }

    pub(super) async fn is_blocked(&self, username: &str, sender_actor: &str) -> Result<bool> {
        let username = username.to_owned();
        let sender_actor = sender_actor.to_owned();
        self.run(move |db| db.is_blocked(&username, &sender_actor))
            .await
    }
    pub(super) async fn upsert_actor_cache(&self, username: &str, actor_json: &str) -> Result<()> {
        let username = username.to_owned();
        let actor_json = actor_json.to_owned();
        self.run(move |db| db.upsert_actor_cache(&username, &actor_json))
            .await
    }
    pub(super) async fn get_actor_cache(&self, username: &str) -> Result<Option<String>> {
        let username = username.to_owned();
        self.run(move |db| db.get_actor_cache(&username)).await
    }
    pub(super) async fn upsert_collection_cache(
        &self,
        username: &str,
        kind: &str,
        json: &str,
    ) -> Result<()> {
        let username = username.to_owned();
        let kind = kind.to_owned();
        let json = json.to_owned();
        self.run(move |db| db.upsert_collection_cache(&username, &kind, &json))
            .await
    }
    pub(super) async fn get_collection_cache(
        &self,
        username: &str,
        kind: &str,
    ) -> Result<Option<String>> {
        let username = username.to_owned();
        let kind = kind.to_owned();
        self.run(move |db| db.get_collection_cache(&username, &kind))
            .await
    }
    pub(super) async fn cleanup_peer_directory(&self, ttl_days: u32) -> Result<u64> {
        self.run(move |db| db.cleanup_peer_directory(ttl_days))
            .await
    }
    pub(super) async fn cleanup_webrtc_signals(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_webrtc_signals(ttl_secs))
            .await
    }
    pub(super) async fn cleanup_admin_audit(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_admin_audit(ttl_secs)).await
    }
    pub(super) async fn cleanup_relay_notes(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_relay_notes(ttl_secs)).await
    }
    pub(super) async fn cleanup_peer_registry(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_peer_registry(ttl_secs)).await
    }
    pub(super) async fn cleanup_relay_registry(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_relay_registry(ttl_secs))
            .await
    }
    pub(super) async fn cleanup_relay_user_directory(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_relay_user_directory(ttl_secs))
            .await
    }
    pub(super) async fn insert_inbox_dedupe(
        &self,
        activity_id: &str,
        recipient: &str,
    ) -> Result<bool> {
        let activity_id = activity_id.to_owned();
        let recipient = recipient.to_owned();
        self.run(move |db| db.insert_inbox_dedupe(&activity_id, &recipient))
            .await
    }
    pub(super) async fn delete_inbox_dedupe(
        &self,
        activity_id: &str,
        recipient: &str,
    ) -> Result<()> {
        let activity_id = activity_id.to_owned();
        let recipient = recipient.to_owned();
        self.run(move |db| db.delete_inbox_dedupe(&activity_id, &recipient))
            .await
    }
    pub(super) async fn cleanup_inbox_dedupe(&self, ttl_secs: u64) -> Result<u64> {
        self.run(move |db| db.cleanup_inbox_dedupe(ttl_secs)).await
    }
    #[cfg(test)]
    pub(super) async fn user_media_bytes(&self, username: &str) -> Result<u64> {
        let username = username.to_owned();
        self.run(move |db| db.user_media_bytes(&username)).await
    }
    pub(super) async fn delete_media_item(&self, username: &str, id: &str) -> Result<bool> {
        let username = username.to_owned();
        let id = id.to_owned();
        self.run(move |db| db.delete_media_item(&username, &id))
            .await
    }
    pub(super) async fn get_media_item(
        &self,
        username: &str,
        id: &str,
    ) -> Result<Option<MediaItem>> {
        let username = username.to_owned();
        let id = id.to_owned();
        self.run(move |db| db.get_media_item(&username, &id)).await
    }
    pub(super) async fn upsert_relay_note(&self, note: &RelayNoteIndex) -> Result<()> {
        let note = note.clone();
        self.run(move |db| db.upsert_relay_note(&note)).await
    }
    pub(super) async fn search_relay_notes(
        &self,
        q: &str,
        tag: &str,
        limit: u32,
        cursor: Option<i64>,
        since: Option<i64>,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let q = q.to_owned();
        let tag = tag.to_owned();
        self.run(move |db| db.search_relay_notes(&q, &tag, limit, cursor, since, total_mode))
            .await
    }
    pub(super) async fn list_relay_notes_sync(
        &self,
        limit: u32,
        since: Option<i64>,
        cursor: Option<i64>,
    ) -> Result<CollectionPage<(String, i64)>> {
        self.run(move |db| db.list_relay_notes_sync(limit, since, cursor))
            .await
    }
    pub(super) async fn search_relay_users(
        &self,
        q: &str,
        limit: u32,
        cursor: Option<i64>,
        base_template: &str,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let q = q.to_owned();
        let base_template = base_template.to_owned();
        self.run(move |db| db.search_relay_users(&q, limit, cursor, &base_template, total_mode))
            .await
    }
    pub(super) async fn search_relay_tags(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<(String, u64)>> {
        let q = q.to_owned();
        self.run(move |db| db.search_relay_tags(&q, limit)).await
    }
    pub(super) async fn upsert_peer_directory(
        &self,
        peer_id: &str,
        username: &str,
        actor_url: &str,
    ) -> Result<()> {
        let peer_id = peer_id.to_owned();
        let username = username.to_owned();
        let actor_url = actor_url.to_owned();
        self.run(move |db| db.upsert_peer_directory(&peer_id, &username, &actor_url))
            .await
    }
    pub(super) async fn list_peer_directory(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String)>> {
        let q = q.to_owned();
        self.run(move |db| db.list_peer_directory(&q, limit)).await
    }
    pub(super) async fn upsert_relay_user_directory(
        &self,
        username: &str,
        actor_url: &str,
        relay_url: &str,
    ) -> Result<()> {
        let username = username.to_owned();
        let actor_url = actor_url.to_owned();
        let relay_url = relay_url.to_owned();
        self.run(move |db| db.upsert_relay_user_directory(&username, &actor_url, &relay_url))
            .await
    }

    pub(super) async fn upsert_outbox_index_state(&self, username: &str, ok: bool) -> Result<()> {
        let username = username.to_owned();
        self.run(move |db| db.upsert_outbox_index_state(&username, ok))
            .await
    }
    pub(super) async fn count_outbox_indexed_since(&self, cutoff_ms: i64) -> Result<u64> {
        self.run(move |db| db.count_outbox_indexed_since(cutoff_ms))
            .await
    }
    pub(super) async fn get_outbox_index_state(
        &self,
        username: &str,
    ) -> Result<Option<(i64, bool)>> {
        let username = username.to_owned();
        self.run(move |db| db.get_outbox_index_state(&username))
            .await
    }
    pub(super) async fn count_webrtc_signals_for_peer(&self, to_peer_id: &str) -> Result<u64> {
        let to_peer_id = to_peer_id.to_owned();
        self.run(move |db| db.count_webrtc_signals_for_peer(&to_peer_id))
            .await
    }
    pub(super) async fn insert_webrtc_signal(
        &self,
        signal_id: &str,
        to_peer_id: &str,
        from_actor: &str,
        session_id: &str,
        kind: &str,
        payload_json: &str,
    ) -> Result<()> {
        let signal_id = signal_id.to_owned();
        let to_peer_id = to_peer_id.to_owned();
        let from_actor = from_actor.to_owned();
        let session_id = session_id.to_owned();
        let kind = kind.to_owned();
        let payload_json = payload_json.to_owned();
        self.run(move |db| {
            db.insert_webrtc_signal(
                &signal_id,
                &to_peer_id,
                &from_actor,
                &session_id,
                &kind,
                &payload_json,
            )
        })
        .await
    }
    pub(super) async fn list_webrtc_signals(
        &self,
        to_peer_id: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let to_peer_id = to_peer_id.to_owned();
        self.run(move |db| db.list_webrtc_signals(&to_peer_id, limit))
            .await
    }
    pub(super) async fn delete_webrtc_signals(
        &self,
        to_peer_id: &str,
        ids: &[String],
    ) -> Result<u64> {
        let to_peer_id = to_peer_id.to_owned();
        let ids = ids.to_vec();
        self.run(move |db| db.delete_webrtc_signals(&to_peer_id, &ids))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn blocked_sql_does_not_block_runtime_or_other_queries() {
        let (db, _dir) = crate::tests::test_db();
        let dispatch = DbDispatch::new(db);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let slow = dispatch.clone();
        let task = tokio::spawn(async move {
            slow.run(move |_| {
                let _ = started_tx.send(());
                release_rx.recv_timeout(Duration::from_secs(3))?;
                Ok(())
            })
            .await
        });
        started_rx.await.unwrap();
        let fast = tokio::time::timeout(Duration::from_secs(1), dispatch.count_users_total()).await;
        release_tx.send(()).unwrap();
        assert_eq!(fast.unwrap().unwrap(), 0);
        task.await.unwrap().unwrap();
    }
}
