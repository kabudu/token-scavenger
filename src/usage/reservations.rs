//! Single-process spend reservations for paid adaptive routing and classification.
//!
//! Historical usage alone is not a concurrent budget. Reservations are taken
//! before a paid call and reconciled when authoritative usage is stored.
//! Unresolved reservations survive restart as retained charges. This is not a
//! provider invoice cap.

use crate::api::error::ApiError;
use dashmap::DashMap;
use sqlx::SqlitePool;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct SpendReservation {
    pub id: i64,
    pub scope_key: String,
    pub amount_micros: i64,
}

#[derive(Debug, Clone)]
pub struct ScopeCeiling {
    pub scope_key: String,
    pub committed_micros: i64,
    pub limit_micros: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ReservationBatch {
    pub items: Vec<SpendReservation>,
}

impl ReservationBatch {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub async fn settle(self, ledger: &BudgetLedger, db: &SqlitePool) {
        for item in &self.items {
            ledger.settle(db, item).await;
        }
    }

    pub async fn retain(self, ledger: &BudgetLedger, db: &SqlitePool) {
        for item in &self.items {
            ledger.retain(db, item).await;
        }
    }
}

pub fn utc_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

pub fn scope_request(request_id: &str) -> String {
    format!("request:{request_id}")
}

pub fn scope_global_day(day: &str) -> String {
    format!("global:{day}")
}

pub fn scope_provider_day(provider_id: &str, day: &str) -> String {
    format!("provider:{provider_id}:{day}")
}

pub fn scope_group_day(requested_model: &str, day: &str) -> String {
    format!("group:{requested_model}:{day}")
}

pub fn scope_project_day(project_id: &str, day: &str) -> String {
    format!("project:{project_id}:{day}")
}

pub fn scope_org_day(organization_id: &str, day: &str) -> String {
    format!("org:{organization_id}:{day}")
}

pub fn scope_env_day(environment: &str, day: &str) -> String {
    format!("env:{environment}:{day}")
}

pub fn scope_key_day(key_prefix: &str, day: &str) -> String {
    format!("key:{key_prefix}:{day}")
}

pub struct BudgetLedger {
    seeded: AtomicBool,
    paid_blocked: AtomicBool,
    /// Serialises seeding so two startups cannot double-count retained rows.
    seed_gate: tokio::sync::Mutex<()>,
    /// Serialises check-and-increment. Never held across await or SQL.
    admit: StdMutex<()>,
    /// Outstanding micros by scope. Includes retained crash charges.
    outstanding: DashMap<String, i64>,
}

impl BudgetLedger {
    pub fn new() -> Self {
        Self {
            seeded: AtomicBool::new(false),
            paid_blocked: AtomicBool::new(true),
            seed_gate: tokio::sync::Mutex::new(()),
            admit: StdMutex::new(()),
            outstanding: DashMap::new(),
        }
    }

    pub fn paid_admission_blocked(&self) -> bool {
        self.paid_blocked.load(Ordering::Relaxed)
    }

    pub fn outstanding_micros(&self, scope: &str) -> i64 {
        self.outstanding.get(scope).map(|value| *value).unwrap_or(0)
    }

    pub async fn ensure_seeded(&self, db: &SqlitePool) -> Result<(), ApiError> {
        if self.seeded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _gate = self.seed_gate.lock().await;
        if self.seeded.load(Ordering::Acquire) {
            return Ok(());
        }
        // A request may have several provider attempts with the same purpose.
        // A usage row for one attempt cannot prove that another reservation was
        // billed or settled. Until reservations and usage share an attempt ID,
        // retain all crash-uncertain charges rather than risk undercounting.
        sqlx::query("UPDATE spend_reservations SET state = 'retained' WHERE state = 'reserved'")
            .execute(db)
            .await
            .map_err(internal)?;
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT scope_key, COALESCE(SUM(amount_micros), 0)
             FROM spend_reservations
             WHERE state = 'retained'
             GROUP BY scope_key",
        )
        .fetch_all(db)
        .await
        .map_err(internal)?;
        for (scope, amount) in rows {
            self.outstanding.insert(scope, amount.max(0));
        }
        self.paid_blocked.store(false, Ordering::Release);
        self.seeded.store(true, Ordering::Release);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reserve(
        &self,
        db: &SqlitePool,
        request_id: &str,
        project_id: &str,
        purpose: &str,
        scope_key: &str,
        amount_micros: i64,
        committed_micros: i64,
        limit_micros: Option<i64>,
    ) -> Result<Option<SpendReservation>, ApiError> {
        let batch = self
            .reserve_many(
                db,
                request_id,
                project_id,
                purpose,
                amount_micros,
                &[ScopeCeiling {
                    scope_key: scope_key.to_string(),
                    committed_micros,
                    limit_micros,
                }],
            )
            .await?;
        Ok(batch.items.into_iter().next())
    }

    /// Admit one amount against every ceiling, or none of them.
    ///
    /// The in-memory check and increment share one mutex and do not await.
    /// The reservation rows are written after that lock is released.
    pub async fn reserve_many(
        &self,
        db: &SqlitePool,
        request_id: &str,
        project_id: &str,
        purpose: &str,
        amount_micros: i64,
        scopes: &[ScopeCeiling],
    ) -> Result<ReservationBatch, ApiError> {
        self.ensure_seeded(db).await?;
        if amount_micros <= 0 || scopes.is_empty() {
            return Ok(ReservationBatch { items: Vec::new() });
        }
        if self.paid_admission_blocked() {
            return Err(admission_blocked());
        }
        {
            let _guard = self.admit.lock().unwrap_or_else(|error| error.into_inner());
            for scope in scopes {
                let projected = scope
                    .committed_micros
                    .saturating_add(self.outstanding_micros(&scope.scope_key))
                    .saturating_add(amount_micros);
                if let Some(limit) = scope.limit_micros {
                    if projected > limit {
                        return Err(ceiling_exceeded(&scope.scope_key));
                    }
                }
            }
            for scope in scopes {
                *self.outstanding.entry(scope.scope_key.clone()).or_insert(0) += amount_micros;
            }
        }
        let mut items = Vec::with_capacity(scopes.len());
        for scope in scopes {
            let inserted = sqlx::query(
                "INSERT INTO spend_reservations
                 (request_id, project_id, purpose, scope_key, amount_micros, state)
                 VALUES (?, ?, ?, ?, ?, 'reserved')",
            )
            .bind(request_id)
            .bind(project_id)
            .bind(purpose)
            .bind(&scope.scope_key)
            .bind(amount_micros)
            .execute(db)
            .await;
            match inserted {
                Ok(done) => items.push(SpendReservation {
                    id: done.last_insert_rowid(),
                    scope_key: scope.scope_key.clone(),
                    amount_micros,
                }),
                Err(error) => {
                    self.release_memory(scopes, amount_micros);
                    for item in &items {
                        let _ = sqlx::query("DELETE FROM spend_reservations WHERE id = ?")
                            .bind(item.id)
                            .execute(db)
                            .await;
                    }
                    return Err(internal(error));
                }
            }
        }
        Ok(ReservationBatch { items })
    }

    fn release_memory(&self, scopes: &[ScopeCeiling], amount_micros: i64) {
        let _guard = self.admit.lock().unwrap_or_else(|error| error.into_inner());
        for scope in scopes {
            if let Some(mut outstanding) = self.outstanding.get_mut(&scope.scope_key) {
                *outstanding = outstanding.saturating_sub(amount_micros);
            }
        }
    }

    pub async fn settle(&self, db: &SqlitePool, reservation: &SpendReservation) {
        let _ = sqlx::query("UPDATE spend_reservations SET state = 'settled' WHERE id = ?")
            .bind(reservation.id)
            .execute(db)
            .await;
        if let Some(mut outstanding) = self.outstanding.get_mut(&reservation.scope_key) {
            *outstanding = outstanding.saturating_sub(reservation.amount_micros);
        }
    }

    pub async fn retain(&self, db: &SqlitePool, reservation: &SpendReservation) {
        let _ = sqlx::query("UPDATE spend_reservations SET state = 'retained' WHERE id = ?")
            .bind(reservation.id)
            .execute(db)
            .await;
    }
}

impl Default for BudgetLedger {
    fn default() -> Self {
        Self::new()
    }
}

pub fn usd_to_micros(usd: f64) -> Option<i64> {
    if !usd.is_finite() || usd < 0.0 {
        return None;
    }
    let micros = (usd * 1_000_000.0).round();
    if micros > i64::MAX as f64 {
        return None;
    }
    Some(micros as i64)
}

pub fn micros_to_usd(micros: i64) -> f64 {
    micros as f64 / 1_000_000.0
}

fn internal(error: sqlx::Error) -> ApiError {
    ApiError::InternalError(error.to_string())
}

fn admission_blocked() -> ApiError {
    ApiError::AgentRouting {
        http_status: 503,
        code: "budget_denied",
        message: "paid adaptive admission is blocked until unresolved spend is reconciled".into(),
        retry_after: None,
    }
}

fn ceiling_exceeded(scope_key: &str) -> ApiError {
    ApiError::AgentRouting {
        http_status: 429,
        code: "budget_denied",
        message: format!("reserved spend for {scope_key} would exceed the configured ceiling"),
        retry_after: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micros_reject_non_finite_values() {
        assert_eq!(usd_to_micros(1.25), Some(1_250_000));
        assert!(usd_to_micros(f64::NAN).is_none());
        assert!(usd_to_micros(-1.0).is_none());
    }

    #[tokio::test]
    async fn retained_reservations_survive_a_new_ledger() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .unwrap();
        let ledger = BudgetLedger::new();
        let reservation = ledger
            .reserve(
                &pool,
                "req-1",
                "default",
                "classification",
                "project:default",
                2_000,
                0,
                Some(10_000),
            )
            .await
            .unwrap()
            .unwrap();
        drop(ledger);
        let restarted = BudgetLedger::new();
        restarted.ensure_seeded(&pool).await.unwrap();
        assert_eq!(restarted.outstanding_micros("project:default"), 2_000);
        assert!(!restarted.paid_admission_blocked());
        let _ = reservation;
    }

    #[tokio::test]
    async fn unrelated_attempt_usage_does_not_clear_a_crash_uncertain_charge() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO request_log (request_id, endpoint_kind, requested_model, status) VALUES ('req', 'chat', 'agent-auto', 'pending')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO usage_events (request_id, provider_id, purpose, estimated_cost_usd) VALUES ('req', 'first-provider', 'inference', 0.001)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spend_reservations (request_id, project_id, purpose, scope_key, amount_micros, state) VALUES ('req', 'default', 'inference', 'global:today', 2000, 'reserved')")
            .execute(&pool)
            .await
            .unwrap();

        let restarted = BudgetLedger::new();
        restarted.ensure_seeded(&pool).await.unwrap();
        assert_eq!(restarted.outstanding_micros("global:today"), 2_000);
    }

    #[tokio::test]
    async fn concurrent_reservations_cannot_both_pass_one_ceiling() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .unwrap();
        let ledger = std::sync::Arc::new(BudgetLedger::new());
        let left = ledger.clone();
        let right = ledger.clone();
        let left_pool = pool.clone();
        let right_pool = pool.clone();
        let (first, second) = tokio::join!(
            left.reserve(
                &left_pool,
                "req-a",
                "default",
                "inference",
                "global:today",
                800,
                0,
                Some(1_000),
            ),
            right.reserve(
                &right_pool,
                "req-b",
                "default",
                "inference",
                "global:today",
                800,
                0,
                Some(1_000),
            ),
        );
        let admitted = [&first, &second]
            .into_iter()
            .filter(|result| {
                result
                    .as_ref()
                    .ok()
                    .and_then(|reservation| reservation.as_ref())
                    .is_some()
            })
            .count();
        let denied = [&first, &second]
            .into_iter()
            .filter(|result| result.as_ref().is_err())
            .count();
        assert_eq!(admitted, 1);
        assert_eq!(denied, 1);
        assert_eq!(ledger.outstanding_micros("global:today"), 800);
    }
}
