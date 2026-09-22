//! Paid adaptive and classifier admission.
//!
//! Reservations are process-local. A configured ceiling rejects an unknown
//! price and counts in-flight reservations plus historical usage. No ceiling
//! means a paid call is not held. Seeding failure blocks paid work.

use crate::api::error::ApiError;
use crate::app::state::AppState;
use crate::usage::pricing_catalog::{PricingUsage, calculate_cost, lookup_rate};
use crate::usage::reservations::{
    ReservationBatch, ScopeCeiling, scope_env_day, scope_global_day, scope_group_day,
    scope_key_day, scope_org_day, scope_project_day, scope_provider_day, scope_request,
    usd_to_micros, utc_day,
};

pub struct PaidHold {
    batch: ReservationBatch,
    state: AppState,
    settled: bool,
}

impl PaidHold {
    fn new(state: AppState, batch: ReservationBatch) -> Self {
        Self {
            batch,
            state,
            settled: false,
        }
    }

    pub async fn settle(mut self) {
        self.settled = true;
        let batch = std::mem::take(&mut self.batch);
        batch
            .settle(&self.state.budget_ledger, &self.state.db)
            .await;
    }
}

impl Drop for PaidHold {
    fn drop(&mut self) {
        if self.settled || self.batch.items.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.batch);
        let state = self.state.clone();
        tokio::spawn(async move {
            batch.retain(&state.budget_ledger, &state.db).await;
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn reserve_adaptive_paid(
    state: &AppState,
    project_id: &str,
    api_key_prefix: &str,
    request_id: &str,
    purpose: &str,
    provider_id: &str,
    model_id: &str,
    requested_model: &str,
    free_only: bool,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<Option<PaidHold>, ApiError> {
    if free_only {
        return Ok(None);
    }
    if let Err(error) = state.budget_ledger.ensure_seeded(&state.db).await {
        tracing::warn!(%error, "paid admission seed failed");
        return Err(admission_blocked());
    }
    if state.budget_ledger.paid_admission_blocked() {
        return Err(admission_blocked());
    }

    let day = utc_day();
    let mut scopes = Vec::new();
    let budgets = &state.config().routing.budgets;
    if let Some(limit) = budgets.max_cost_per_request_usd {
        let committed = request_spend_micros(state, request_id).await?;
        push_ceiling(&mut scopes, scope_request(request_id), committed, limit)?;
    }
    if let Some(limit) = budgets.max_cost_per_day_usd {
        let committed = spend_micros_unbound(
            state,
            "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
             FROM usage_events
             WHERE timestamp >= date('now')",
        )
        .await?;
        push_ceiling(&mut scopes, scope_global_day(&day), committed, limit)?;
    }
    if let Some(limit) = budgets
        .max_cost_per_provider_per_day_usd
        .get(provider_id)
        .copied()
    {
        let committed = spend_micros_one(
            state,
            "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
             FROM usage_events
             WHERE provider_id = ? AND timestamp >= date('now')",
            provider_id,
        )
        .await?;
        push_ceiling(
            &mut scopes,
            scope_provider_day(provider_id, &day),
            committed,
            limit,
        )?;
    }
    if let Some(limit) = budgets
        .max_cost_per_model_group_per_day_usd
        .get(requested_model)
        .copied()
    {
        let committed = group_spend_micros(state, requested_model).await?;
        push_ceiling(
            &mut scopes,
            scope_group_day(requested_model, &day),
            committed,
            limit,
        )?;
    }

    if let Some(policy) = crate::projects::load_project_policy(&state.db, project_id).await? {
        if let Some(limit) = policy.max_cost_per_request_usd {
            let committed = request_spend_micros(state, request_id).await?;
            push_ceiling(&mut scopes, scope_request(request_id), committed, limit)?;
        }
        if let Some(limit) = policy.max_cost_per_day_usd {
            let committed = spend_micros_one(
                state,
                "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
                 FROM usage_events
                 WHERE project_id = ? AND timestamp >= date('now')",
                project_id,
            )
            .await?;
            push_ceiling(
                &mut scopes,
                scope_project_day(project_id, &day),
                committed,
                limit,
            )?;
        }
        if let (Some(org_id), Some(limit)) = (
            policy.organization_id.as_deref(),
            policy.max_cost_per_org_per_day_usd,
        ) {
            let committed = scoped_project_spend_micros(state, "organization_id", org_id).await?;
            push_ceiling(&mut scopes, scope_org_day(org_id, &day), committed, limit)?;
        }
        if let (Some(environment), Some(limit)) = (
            policy.environment.as_deref(),
            policy.max_cost_per_environment_per_day_usd,
        ) {
            let committed = scoped_project_spend_micros(state, "environment", environment).await?;
            push_ceiling(
                &mut scopes,
                scope_env_day(environment, &day),
                committed,
                limit,
            )?;
        }
    }

    if !api_key_prefix.is_empty() && api_key_prefix != "master" {
        if let Some(limit) = key_cost_limit(state, api_key_prefix).await? {
            let committed = spend_micros_one(
                state,
                "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
                 FROM usage_events
                 WHERE api_key_prefix = ? AND timestamp >= date('now')",
                api_key_prefix,
            )
            .await?;
            push_ceiling(
                &mut scopes,
                scope_key_day(api_key_prefix, &day),
                committed,
                limit,
            )?;
        }
    }

    if scopes.is_empty() {
        return Ok(None);
    }

    let usage = PricingUsage {
        input_tokens,
        output_tokens,
        ..PricingUsage::default()
    };
    let rate = lookup_rate(&state.db, provider_id, model_id)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
    let Some(rate) = rate else {
        return Err(unknown_price());
    };
    let estimate = calculate_cost(&rate, &usage);
    let Some(amount_micros) = usd_to_micros(estimate.amount_usd) else {
        return Err(unknown_price());
    };
    let batch = state
        .budget_ledger
        .reserve_many(
            &state.db,
            request_id,
            project_id,
            purpose,
            amount_micros,
            &scopes,
        )
        .await?;
    if batch.is_empty() {
        return Ok(None);
    }
    Ok(Some(PaidHold::new(state.clone(), batch)))
}

fn push_ceiling(
    scopes: &mut Vec<ScopeCeiling>,
    scope_key: String,
    committed_micros: i64,
    limit_usd: f64,
) -> Result<(), ApiError> {
    let Some(limit_micros) = usd_to_micros(limit_usd) else {
        return Err(ApiError::InvalidRequest(
            "budget ceiling is not a finite USD amount".into(),
        ));
    };
    if let Some(existing) = scopes.iter_mut().find(|scope| scope.scope_key == scope_key) {
        existing.limit_micros = Some(
            existing
                .limit_micros
                .map(|limit| limit.min(limit_micros))
                .unwrap_or(limit_micros),
        );
        existing.committed_micros = existing.committed_micros.max(committed_micros);
        return Ok(());
    }
    scopes.push(ScopeCeiling {
        scope_key,
        committed_micros,
        limit_micros: Some(limit_micros),
    });
    Ok(())
}

async fn request_spend_micros(state: &AppState, request_id: &str) -> Result<i64, ApiError> {
    spend_micros_one(
        state,
        "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
         FROM usage_events
         WHERE request_id = ?",
        request_id,
    )
    .await
}

async fn group_spend_micros(state: &AppState, requested_model: &str) -> Result<i64, ApiError> {
    let usd = sqlx::query_as::<_, (f64,)>(
        "SELECT COALESCE(SUM(usage_events.estimated_cost_usd), 0.0)
         FROM usage_events
         JOIN request_log USING (request_id)
         WHERE usage_events.timestamp >= date('now')
           AND request_log.requested_model = ?",
    )
    .bind(requested_model)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?
    .map(|row| row.0)
    .unwrap_or(0.0);
    finite_micros(usd)
}

async fn scoped_project_spend_micros(
    state: &AppState,
    column: &str,
    scope_id: &str,
) -> Result<i64, ApiError> {
    let column = match column {
        "organization_id" => "organization_id",
        "environment" => "environment",
        _ => return Ok(0),
    };
    let usd = sqlx::query_as::<_, (f64,)>(&format!(
        "SELECT COALESCE(SUM(u.estimated_cost_usd), 0.0)
         FROM usage_events u
         JOIN projects p ON p.project_id = u.project_id
         WHERE p.{column} = ? AND u.timestamp >= date('now')"
    ))
    .bind(scope_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?
    .map(|row| row.0)
    .unwrap_or(0.0);
    finite_micros(usd)
}

async fn key_cost_limit(state: &AppState, api_key_prefix: &str) -> Result<Option<f64>, ApiError> {
    sqlx::query_as::<_, (Option<f64>,)>(
        "SELECT max_cost_per_day_usd FROM project_api_keys WHERE key_prefix = ?",
    )
    .bind(api_key_prefix)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))
    .map(|row| row.and_then(|row| row.0))
}

async fn spend_micros_unbound(state: &AppState, sql: &str) -> Result<i64, ApiError> {
    let usd = sqlx::query_as::<_, (f64,)>(sql)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
        .map(|row| row.0)
        .unwrap_or(0.0);
    finite_micros(usd)
}

async fn spend_micros_one(state: &AppState, sql: &str, bind: &str) -> Result<i64, ApiError> {
    let usd = sqlx::query_as::<_, (f64,)>(sql)
        .bind(bind)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
        .map(|row| row.0)
        .unwrap_or(0.0);
    finite_micros(usd)
}

fn finite_micros(usd: f64) -> Result<i64, ApiError> {
    usd_to_micros(usd).ok_or_else(|| {
        ApiError::InternalError("historical spend is not a finite USD amount".into())
    })
}

fn admission_blocked() -> ApiError {
    ApiError::AgentRouting {
        http_status: 503,
        code: "budget_denied",
        message: "paid adaptive admission is blocked until unresolved spend is reconciled".into(),
        retry_after: None,
    }
}

fn unknown_price() -> ApiError {
    ApiError::AgentRouting {
        http_status: 429,
        code: "budget_denied",
        message: "paid call has no price and a hard budget ceiling applies".into(),
        retry_after: None,
    }
}
