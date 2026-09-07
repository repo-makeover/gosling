//! The session turn lease: one writer per session, and the rules under which
//! that writer loses the session to someone else (REL-GSL-005).
//!
//! The lease answers "may this process start a turn on this session?" A turn
//! renews its lease every [`LEASE_HEARTBEAT_INTERVAL`], and that heartbeat is
//! the *only* way a long turn proves it is still alive. Nothing else works:
//! elapsed time says nothing (a research turn legitimately runs for hours), and
//! neither does message activity (a turn can sit on a single provider call for
//! minutes). A turn that stops heartbeating for [`LEASE_TTL`] has stopped
//! proving anything.
//!
//! **A live owner's lease can expire.** The owning process being alive is not
//! proof that its turn is: the 2026-09-05 write-gate deadlock left `gosling
//! serve` running, and responsive to a process probe, while its turn had been
//! frozen for hours. So process liveness cannot be a precondition for takeover
//! -- if it were, the only way to recover a wedged session would be killing the
//! app, which is exactly what that incident required. Liveness still narrows
//! the window: a lease whose owner is *gone* is free immediately, because
//! nothing can still be executing and there is nothing to wait for.
//!
//! What makes taking a live owner's lease safe is that expiry is **fenced**.
//! Takeover deletes the old lease row, so the old owner's next heartbeat
//! updates zero rows. That signal is unambiguous -- only a takeover or an
//! explicit release removes the row -- and the old owner responds by cancelling
//! its own turn. A session that changes hands therefore never has two turns
//! writing to it at once, which is what the lease exists to prevent. If the
//! eviction was premature, the cost is one abandoned turn, not a corrupted
//! session.
//!
//! A renewal that *fails* is not revocation. An unreachable database says
//! nothing about who owns the lease, so the heartbeat keeps trying.

use super::SessionStorage;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

const LEASE_TTL: Duration = Duration::from_secs(90);
const LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

pub(crate) struct SessionTurnLease {
    storage: Arc<SessionStorage>,
    session_id: String,
    lease_id: String,
    heartbeat_cancel: CancellationToken,
    turn_cancel: CancellationToken,
    released: bool,
}

impl SessionTurnLease {
    /// The token the turn holding this lease must run under.
    ///
    /// It is cancelled when the caller's own token is cancelled (it is that
    /// token's child) and when the lease is revoked by another process taking
    /// the session over.
    pub(crate) fn turn_cancel_token(&self) -> CancellationToken {
        self.turn_cancel.clone()
    }

    /// Runs one heartbeat synchronously. The spawned heartbeat only fires
    /// every [`LEASE_HEARTBEAT_INTERVAL`], which is far too long to wait for in
    /// a test, so revocation fencing is exercised through this instead.
    #[cfg(test)]
    pub(crate) async fn heartbeat_once(&self) -> bool {
        heartbeat_tick(
            &self.storage,
            &self.session_id,
            &self.lease_id,
            &self.turn_cancel,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn release(mut self) -> Result<()> {
        self.heartbeat_cancel.cancel();
        self.storage
            .release_session_turn_lease(&self.session_id, &self.lease_id)
            .await?;
        self.released = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn abandon(mut self) {
        self.heartbeat_cancel.cancel();
        self.released = true;
    }
}

impl Drop for SessionTurnLease {
    fn drop(&mut self) {
        self.heartbeat_cancel.cancel();
        if self.released {
            return;
        }
        let storage = Arc::clone(&self.storage);
        let session_id = self.session_id.clone();
        let lease_id = self.lease_id.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = storage
                    .release_session_turn_lease(&session_id, &lease_id)
                    .await;
            });
        }
    }
}

/// One heartbeat: renew, and fence the turn off if the lease is gone.
/// Returns whether the heartbeat should keep running.
async fn heartbeat_tick(
    storage: &SessionStorage,
    session_id: &str,
    lease_id: &str,
    turn_cancel: &CancellationToken,
) -> bool {
    match storage.renew_session_turn_lease(session_id, lease_id).await {
        Ok(LeaseRenewal::Held) => true,
        Ok(LeaseRenewal::Revoked) => {
            tracing::warn!(
                session.id = session_id,
                lease.id = lease_id,
                "session turn lease was taken over by another process; cancelling this turn \
                 so the session keeps a single writer"
            );
            turn_cancel.cancel();
            false
        }
        // A renewal that could not run at all says nothing about who owns the
        // lease -- an unreachable database is not a takeover. Keep trying; the
        // owner still has until the live-owner grace to recover.
        Err(error) => {
            tracing::debug!(
                session.id = session_id,
                "turn lease renewal failed: {error}"
            );
            true
        }
    }
}

/// Whether a heartbeat still owned the lease it tried to renew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaseRenewal {
    Held,
    Revoked,
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl SessionStorage {
    pub(super) async fn acquire_session_turn_lease(
        self: Arc<Self>,
        session_id: &str,
        parent_cancel: Option<&CancellationToken>,
    ) -> Result<SessionTurnLease> {
        let pool = self.pool().await?;
        let lease_id = loop {
            let observed = sqlx::query_as::<_, (String, i64, i64)>(
                "SELECT lease_id, owner_pid, updated_at FROM session_turn_leases WHERE session_id = ?",
            )
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
            let now = unix_timestamp();
            let owner_is_live = match observed.as_ref() {
                Some((_, owner_pid, _)) => match u32::try_from(*owner_pid) {
                    Ok(pid) => crate::subprocess::process_is_alive(pid).await,
                    Err(_) => false,
                },
                None => false,
            };
            let heartbeat_is_fresh = observed
                .as_ref()
                .is_some_and(|(_, _, updated_at)| *updated_at >= now - LEASE_TTL.as_secs() as i64);
            let lease_is_held = owner_is_live && heartbeat_is_fresh;

            let write_guard = self.acquire_write_guard().await;
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = sqlx::query_as::<_, (String, i64, i64)>(
                "SELECT lease_id, owner_pid, updated_at FROM session_turn_leases WHERE session_id = ?",
            )
            .bind(session_id)
            .fetch_optional(&mut *tx)
            .await?;
            if current != observed {
                tx.rollback().await?;
                drop(write_guard);
                continue;
            }
            if lease_is_held {
                tx.rollback().await?;
                anyhow::bail!(
                    "session {session_id} already has an active turn in another Gosling process or window"
                );
            }
            if let Some((existing_lease_id, _, _)) = current {
                // Taking the session from an owner that never released it. The
                // previous owner is fenced off by this DELETE: its next
                // heartbeat finds no row and cancels its turn.
                tracing::warn!(
                    session.id = session_id,
                    lease.id = existing_lease_id.as_str(),
                    lease.owner_was_live = owner_is_live,
                    "taking over a session turn lease whose owner stopped renewing it"
                );
                sqlx::query(
                    "DELETE FROM session_turn_leases WHERE session_id = ? AND lease_id = ?",
                )
                .bind(session_id)
                .bind(existing_lease_id)
                .execute(&mut *tx)
                .await?;
            }

            let lease_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO session_turn_leases (
                    session_id, lease_id, owner_id, owner_pid, acquired_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(session_id)
            .bind(&lease_id)
            .bind(&self.owner_id)
            .bind(std::process::id() as i64)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            drop(write_guard);
            break lease_id;
        };

        let heartbeat_cancel = CancellationToken::new();
        let turn_cancel = match parent_cancel {
            Some(parent) => parent.child_token(),
            None => CancellationToken::new(),
        };
        let heartbeat_storage = Arc::clone(&self);
        let heartbeat_session_id = session_id.to_string();
        let heartbeat_lease_id = lease_id.clone();
        let heartbeat_stop = heartbeat_cancel.clone();
        let revoked_turn_cancel = turn_cancel.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LEASE_HEARTBEAT_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = heartbeat_stop.cancelled() => break,
                    _ = interval.tick() => {
                        if !heartbeat_tick(
                            &heartbeat_storage,
                            &heartbeat_session_id,
                            &heartbeat_lease_id,
                            &revoked_turn_cancel,
                        )
                        .await
                        {
                            break;
                        }
                    }
                }
            }
        });

        Ok(SessionTurnLease {
            storage: self,
            session_id: session_id.to_string(),
            lease_id,
            heartbeat_cancel,
            turn_cancel,
            released: false,
        })
    }

    async fn renew_session_turn_lease(
        &self,
        session_id: &str,
        lease_id: &str,
    ) -> Result<LeaseRenewal> {
        let _write_guard = self.acquire_write_guard().await;
        let renewed = sqlx::query(
            "UPDATE session_turn_leases SET updated_at = ? WHERE session_id = ? AND lease_id = ?",
        )
        .bind(unix_timestamp())
        .bind(session_id)
        .bind(lease_id)
        .execute(self.pool().await?)
        .await?;
        Ok(if renewed.rows_affected() == 0 {
            LeaseRenewal::Revoked
        } else {
            LeaseRenewal::Held
        })
    }

    /// The `owner_id` of the turn currently running on `session_id`, if any.
    ///
    /// "Currently running" is exactly the test
    /// [`SessionStorage::acquire_session_turn_lease`] uses to decide whether a
    /// session is taken: a heartbeat inside [`LEASE_TTL`] *and* an owner
    /// process that still exists. Read through a caller-supplied transaction so
    /// the answer is consistent with whatever else that transaction decides.
    pub(super) async fn live_turn_owner(
        &self,
        tx: &mut sqlx::SqliteConnection,
        session_id: &str,
    ) -> Result<Option<String>> {
        let observed = sqlx::query_as::<_, (String, i64, i64)>(
            "SELECT owner_id, owner_pid, updated_at FROM session_turn_leases WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((owner_id, owner_pid, updated_at)) = observed else {
            return Ok(None);
        };
        if updated_at < unix_timestamp() - LEASE_TTL.as_secs() as i64 {
            return Ok(None);
        }
        let owner_is_live = match u32::try_from(owner_pid) {
            Ok(pid) => crate::subprocess::process_is_alive(pid).await,
            Err(_) => false,
        };
        Ok(owner_is_live.then_some(owner_id))
    }

    async fn release_session_turn_lease(&self, session_id: &str, lease_id: &str) -> Result<()> {
        let _write_guard = self.acquire_write_guard().await;
        sqlx::query("DELETE FROM session_turn_leases WHERE session_id = ? AND lease_id = ?")
            .bind(session_id)
            .bind(lease_id)
            .execute(self.pool().await?)
            .await?;
        Ok(())
    }
}
