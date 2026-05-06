use regex_lite::Regex;
use sqlx::SqlitePool;

const DEEPSEEK_PRICING_URL: &str = "https://api-docs.deepseek.com/quick_start/pricing/";

#[derive(Debug, Clone)]
pub struct PricingRate {
    pub id: Option<i64>,
    pub provider_id: String,
    pub model_id: String,
    pub input_per_1m: Option<f64>,
    pub cached_input_per_1m: Option<f64>,
    pub output_per_1m: Option<f64>,
    pub reasoning_per_1m: Option<f64>,
    pub confidence: String,
    pub source_kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct PricingUsage {
    pub input_tokens: u32,
    pub cached_input_tokens: Option<u32>,
    pub cache_miss_input_tokens: Option<u32>,
    pub output_tokens: u32,
    pub reasoning_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub amount_usd: f64,
    pub confidence: String,
    pub pricing_model_id: Option<i64>,
    pub formula_json: serde_json::Value,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinRate {
    provider_id: &'static str,
    model_id: &'static str,
    input_per_1m: Option<f64>,
    cached_input_per_1m: Option<f64>,
    output_per_1m: Option<f64>,
    reasoning_per_1m: Option<f64>,
    confidence: &'static str,
    source_kind: &'static str,
    source_url: &'static str,
}

enum FetchResult {
    NotModified,
    Updated {
        rates: Vec<PricingRate>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

const BUILTIN_RATES: &[BuiltinRate] = &[
    BuiltinRate {
        provider_id: "deepseek",
        model_id: "deepseek-chat",
        input_per_1m: Some(0.14),
        cached_input_per_1m: Some(0.028),
        output_per_1m: Some(0.28),
        reasoning_per_1m: None,
        confidence: "provider_published",
        source_kind: "builtin",
        source_url: "https://api-docs.deepseek.com/quick_start/pricing/",
    },
    BuiltinRate {
        provider_id: "deepseek",
        model_id: "deepseek-reasoner",
        input_per_1m: Some(0.14),
        cached_input_per_1m: Some(0.028),
        output_per_1m: Some(0.28),
        reasoning_per_1m: None,
        confidence: "provider_published",
        source_kind: "builtin",
        source_url: "https://api-docs.deepseek.com/quick_start/pricing/",
    },
    BuiltinRate {
        provider_id: "deepseek",
        model_id: "deepseek-v4-flash",
        input_per_1m: Some(0.14),
        cached_input_per_1m: Some(0.028),
        output_per_1m: Some(0.28),
        reasoning_per_1m: None,
        confidence: "provider_published",
        source_kind: "builtin",
        source_url: "https://api-docs.deepseek.com/quick_start/pricing/",
    },
    BuiltinRate {
        provider_id: "deepseek",
        model_id: "deepseek-v4-pro",
        input_per_1m: Some(1.74),
        cached_input_per_1m: Some(0.145),
        output_per_1m: Some(3.48),
        reasoning_per_1m: None,
        confidence: "provider_published",
        source_kind: "builtin",
        source_url: "https://api-docs.deepseek.com/quick_start/pricing/",
    },
    BuiltinRate {
        provider_id: "xai",
        model_id: "grok-4",
        input_per_1m: Some(3.0),
        cached_input_per_1m: Some(0.75),
        output_per_1m: Some(15.0),
        reasoning_per_1m: None,
        confidence: "fallback_estimate",
        source_kind: "builtin",
        source_url: "https://docs.x.ai/docs/models",
    },
    BuiltinRate {
        provider_id: "xai",
        model_id: "grok-3",
        input_per_1m: Some(3.0),
        cached_input_per_1m: None,
        output_per_1m: Some(15.0),
        reasoning_per_1m: None,
        confidence: "fallback_estimate",
        source_kind: "builtin",
        source_url: "https://docs.x.ai/docs/models",
    },
    BuiltinRate {
        provider_id: "xai",
        model_id: "grok-3-mini",
        input_per_1m: Some(0.3),
        cached_input_per_1m: None,
        output_per_1m: Some(0.5),
        reasoning_per_1m: None,
        confidence: "fallback_estimate",
        source_kind: "builtin",
        source_url: "https://docs.x.ai/docs/models",
    },
];

pub async fn seed_builtin_pricing(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    for rate in BUILTIN_RATES {
        sqlx::query(
            "INSERT INTO pricing_sources
             (provider_id, source_kind, source_url, last_checked_at, last_success_at, status)
             VALUES (?, ?, ?, datetime('now'), datetime('now'), 'ok')
             ON CONFLICT(provider_id, source_kind, source_url) DO UPDATE SET
                last_checked_at = excluded.last_checked_at,
                last_success_at = excluded.last_success_at,
                status = 'ok'",
        )
        .bind(rate.provider_id)
        .bind(rate.source_kind)
        .bind(rate.source_url)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT OR IGNORE INTO model_pricing
             (provider_id, model_id, currency, input_per_1m, cached_input_per_1m, output_per_1m, reasoning_per_1m, source_kind, source_url, confidence, fetched_at)
             VALUES (?, ?, 'USD', ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(rate.provider_id)
        .bind(rate.model_id)
        .bind(rate.input_per_1m)
        .bind(rate.cached_input_per_1m)
        .bind(rate.output_per_1m)
        .bind(rate.reasoning_per_1m)
        .bind(rate.source_kind)
        .bind(rate.source_url)
        .bind(rate.confidence)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn refresh_pricing_sources(
    pool: &SqlitePool,
    client: &reqwest::Client,
    force: bool,
) -> Result<serde_json::Value, sqlx::Error> {
    seed_builtin_pricing(pool).await?;

    let mut refreshed = Vec::new();
    let should_fetch_deepseek =
        force || pricing_source_is_stale(pool, "deepseek", "scraped_html").await?;

    if should_fetch_deepseek {
        let (etag, last_modified) =
            pricing_source_validators(pool, "deepseek", "scraped_html").await?;
        match fetch_deepseek_pricing(client, etag.as_deref(), last_modified.as_deref()).await {
            Ok(FetchResult::Updated {
                rates,
                etag,
                last_modified,
            }) => {
                upsert_scraped_rates(pool, &rates).await?;
                mark_pricing_source_ok(
                    pool,
                    "deepseek",
                    "scraped_html",
                    DEEPSEEK_PRICING_URL,
                    etag.as_deref(),
                    last_modified.as_deref(),
                )
                .await?;
                crate::metrics::prometheus::record_pricing_refresh("deepseek", "success");
                crate::metrics::prometheus::record_pricing_age("deepseek", 0.0);
                refreshed.push(serde_json::json!({"provider_id": "deepseek", "status": "success", "rates": rates.len()}));
            }
            Ok(FetchResult::NotModified) => {
                mark_pricing_source_checked(pool, "deepseek", "scraped_html", DEEPSEEK_PRICING_URL)
                    .await?;
                crate::metrics::prometheus::record_pricing_refresh("deepseek", "not_modified");
                crate::metrics::prometheus::record_pricing_age("deepseek", 0.0);
                refreshed.push(serde_json::json!({"provider_id": "deepseek", "status": "not_modified", "rates": 0}));
            }
            Err(error) => {
                mark_pricing_source_error(
                    pool,
                    "deepseek",
                    "scraped_html",
                    DEEPSEEK_PRICING_URL,
                    &error.to_string(),
                )
                .await?;
                crate::metrics::prometheus::record_pricing_refresh("deepseek", "error");
                refreshed.push(serde_json::json!({"provider_id": "deepseek", "status": "error", "error": error.to_string()}));
            }
        }
    } else if let Some(age) = pricing_source_age_seconds(pool, "deepseek", "scraped_html").await? {
        crate::metrics::prometheus::record_pricing_age("deepseek", age);
    }

    Ok(serde_json::json!({"refreshed": refreshed, "force": force}))
}

async fn fetch_deepseek_pricing(
    client: &reqwest::Client,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<FetchResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut request = client.get(DEEPSEEK_PRICING_URL);
    if let Some(etag) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }

    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(FetchResult::NotModified);
    }
    let response = response.error_for_status()?;
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let html = response.text().await?;

    Ok(FetchResult::Updated {
        rates: parse_deepseek_pricing_html(&html)?,
        etag,
        last_modified,
    })
}

pub fn parse_deepseek_pricing_html(
    html: &str,
) -> Result<Vec<PricingRate>, Box<dyn std::error::Error + Send + Sync>> {
    let row_re = Regex::new(r"(?is)<tr[^>]*>(.*?)</tr>")?;
    let money_re = Regex::new(r"\$\s*([0-9]+(?:\.[0-9]+)?)")?;
    let mut rates = Vec::new();

    for row in row_re.captures_iter(html) {
        let text = normalize_html_text(row.get(1).map(|m| m.as_str()).unwrap_or(""));
        let lower = text.to_ascii_lowercase();
        let model_id = if lower.contains("deepseek-chat") {
            Some("deepseek-chat")
        } else if lower.contains("deepseek-reasoner") {
            Some("deepseek-reasoner")
        } else if lower.contains("v4 pro") || lower.contains("v4-pro") {
            Some("deepseek-v4-pro")
        } else if lower.contains("v4") || lower.contains("flash") {
            Some("deepseek-v4-flash")
        } else {
            None
        };

        let Some(model_id) = model_id else {
            continue;
        };

        let prices = money_re
            .captures_iter(&text)
            .filter_map(|cap| cap.get(1).and_then(|m| m.as_str().parse::<f64>().ok()))
            .collect::<Vec<_>>();

        if prices.len() < 3 {
            continue;
        }

        rates.push(PricingRate {
            id: None,
            provider_id: "deepseek".into(),
            model_id: model_id.into(),
            cached_input_per_1m: prices.first().copied(),
            input_per_1m: prices.get(1).copied(),
            output_per_1m: prices.get(2).copied(),
            reasoning_per_1m: None,
            confidence: "scraped".into(),
            source_kind: "scraped_html".into(),
        });
    }

    rates.sort_by(|a, b| a.model_id.cmp(&b.model_id));
    rates.dedup_by(|a, b| a.model_id == b.model_id);

    if rates.is_empty() {
        Err("no DeepSeek pricing rows found".into())
    } else {
        Ok(rates)
    }
}

fn normalize_html_text(html: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").expect("valid tag regex");
    tag_re
        .replace_all(html, " ")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

async fn upsert_scraped_rates(pool: &SqlitePool, rates: &[PricingRate]) -> Result<(), sqlx::Error> {
    for rate in rates {
        // Compare with the current active row; skip if rates are identical.
        if let Some(current) = fetch_rate(pool, &rate.provider_id, &rate.model_id).await? {
            if current.input_per_1m == rate.input_per_1m
                && current.cached_input_per_1m == rate.cached_input_per_1m
                && current.output_per_1m == rate.output_per_1m
                && current.reasoning_per_1m == rate.reasoning_per_1m
            {
                continue;
            }
        }

        sqlx::query(
            "UPDATE model_pricing
             SET effective_until = datetime('now')
             WHERE provider_id = ? AND model_id = ? AND source_kind = 'scraped_html' AND effective_until IS NULL",
        )
        .bind(&rate.provider_id)
        .bind(&rate.model_id)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO model_pricing
             (provider_id, model_id, currency, input_per_1m, cached_input_per_1m, output_per_1m, reasoning_per_1m, source_kind, source_url, confidence, fetched_at)
             VALUES (?, ?, 'USD', ?, ?, ?, ?, 'scraped_html', ?, ?, datetime('now'))",
        )
        .bind(&rate.provider_id)
        .bind(&rate.model_id)
        .bind(rate.input_per_1m)
        .bind(rate.cached_input_per_1m)
        .bind(rate.output_per_1m)
        .bind(rate.reasoning_per_1m)
        .bind(DEEPSEEK_PRICING_URL)
        .bind(&rate.confidence)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn pricing_source_is_stale(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
) -> Result<bool, sqlx::Error> {
    let age = pricing_source_age_seconds(pool, provider_id, source_kind).await?;
    Ok(age.map(|age| age > 86_400.0).unwrap_or(true))
}

async fn pricing_source_age_seconds(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
) -> Result<Option<f64>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<f64>,)>(
        "SELECT (julianday('now') - julianday(last_success_at)) * 86400.0
         FROM pricing_sources
         WHERE provider_id = ? AND source_kind = ? AND last_success_at IS NOT NULL
         ORDER BY last_success_at DESC
         LIMIT 1",
    )
    .bind(provider_id)
    .bind(source_kind)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| r.0))
}

async fn pricing_source_validators(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
) -> Result<(Option<String>, Option<String>), sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT etag, last_modified
         FROM pricing_sources
         WHERE provider_id = ? AND source_kind = ?
         ORDER BY last_checked_at DESC
         LIMIT 1",
    )
    .bind(provider_id)
    .bind(source_kind)
    .fetch_optional(pool)
    .await?;

    Ok(row.unwrap_or((None, None)))
}

async fn mark_pricing_source_ok(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
    source_url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pricing_sources
         (provider_id, source_kind, source_url, etag, last_modified, last_checked_at, last_success_at, last_error_at, last_error_summary, status)
         VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'), NULL, NULL, 'ok')
         ON CONFLICT(provider_id, source_kind, source_url) DO UPDATE SET
            last_checked_at = excluded.last_checked_at,
            last_success_at = excluded.last_success_at,
            etag = COALESCE(excluded.etag, pricing_sources.etag),
            last_modified = COALESCE(excluded.last_modified, pricing_sources.last_modified),
            last_error_at = NULL,
            last_error_summary = NULL,
            status = 'ok'",
    )
    .bind(provider_id)
    .bind(source_kind)
    .bind(source_url)
    .bind(etag)
    .bind(last_modified)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_pricing_source_checked(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
    source_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pricing_sources
         (provider_id, source_kind, source_url, last_checked_at, status)
         VALUES (?, ?, ?, datetime('now'), 'ok')
         ON CONFLICT(provider_id, source_kind, source_url) DO UPDATE SET
            last_checked_at = excluded.last_checked_at,
            status = 'ok'",
    )
    .bind(provider_id)
    .bind(source_kind)
    .bind(source_url)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_pricing_source_error(
    pool: &SqlitePool,
    provider_id: &str,
    source_kind: &str,
    source_url: &str,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pricing_sources
         (provider_id, source_kind, source_url, last_checked_at, last_error_at, last_error_summary, status)
         VALUES (?, ?, ?, datetime('now'), datetime('now'), ?, 'error')
         ON CONFLICT(provider_id, source_kind, source_url) DO UPDATE SET
            last_checked_at = excluded.last_checked_at,
            last_error_at = excluded.last_error_at,
            last_error_summary = excluded.last_error_summary,
            status = 'error'",
    )
    .bind(provider_id)
    .bind(source_kind)
    .bind(source_url)
    .bind(error.chars().take(500).collect::<String>())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_pricing_state(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<f64>, Option<f64>, Option<f64>, Option<f64>, String, String, Option<String>, Option<String>)>(
        "SELECT id, provider_id, model_id, input_per_1m, cached_input_per_1m, output_per_1m, reasoning_per_1m, confidence, source_kind, source_url, fetched_at
         FROM (
             SELECT *, ROW_NUMBER() OVER (
                 PARTITION BY provider_id, model_id
                 ORDER BY CASE source_kind
                     WHEN 'operator_override' THEN 0
                     WHEN 'fetched_structured' THEN 1
                     WHEN 'scraped_html' THEN 2
                     ELSE 3
                 END, id DESC
             ) AS rn
             FROM model_pricing
             WHERE effective_until IS NULL
         )
         WHERE rn = 1
         ORDER BY provider_id, model_id",
    )
    .fetch_all(pool)
    .await;

    let sources = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Option<String>, String)>(
        "SELECT provider_id, source_kind, source_url, last_checked_at, last_success_at, last_error_summary, status
         FROM pricing_sources
         ORDER BY provider_id, source_kind",
    )
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => serde_json::json!({
            "rates": rows.into_iter().map(|(id, provider_id, model_id, input, cached, output, reasoning, confidence, source_kind, source_url, fetched_at)| {
                serde_json::json!({
                    "id": id,
                    "provider_id": provider_id,
                    "model_id": model_id,
                    "input_per_1m": input,
                    "cached_input_per_1m": cached,
                    "output_per_1m": output,
                    "reasoning_per_1m": reasoning,
                    "confidence": confidence,
                    "source_kind": source_kind,
                    "source_url": source_url,
                    "fetched_at": fetched_at,
                })
            }).collect::<Vec<_>>()
            ,
            "sources": sources.unwrap_or_default().into_iter().map(|(provider_id, source_kind, source_url, last_checked_at, last_success_at, last_error_summary, status)| {
                serde_json::json!({
                    "provider_id": provider_id,
                    "source_kind": source_kind,
                    "source_url": source_url,
                    "last_checked_at": last_checked_at,
                    "last_success_at": last_success_at,
                    "last_error_summary": last_error_summary,
                    "status": status,
                })
            }).collect::<Vec<_>>()
        }),
        Err(_) => serde_json::json!({"rates": []}),
    }
}

pub async fn backfill_zero_cost_paid_usage(
    pool: &SqlitePool,
    dry_run: bool,
) -> Result<serde_json::Value, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i64, String, String, i64, i64, Option<i64>, Option<i64>, Option<i64>)>(
        "SELECT id, provider_id, COALESCE(model_id, ''), input_tokens, output_tokens, cached_input_tokens, cache_miss_input_tokens, reasoning_tokens
         FROM usage_events
         WHERE free_tier = 0 AND estimated_cost_usd = 0.0",
    )
    .fetch_all(pool)
    .await?;

    let mut eligible = 0_i64;
    let mut unknown = 0_i64;
    let mut estimated_total = 0.0_f64;

    for (
        id,
        provider_id,
        model_id,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_miss_input_tokens,
        reasoning_tokens,
    ) in rows
    {
        let usage = PricingUsage {
            input_tokens: input_tokens.max(0) as u32,
            cached_input_tokens: cached_input_tokens.map(|v| v.max(0) as u32),
            cache_miss_input_tokens: cache_miss_input_tokens.map(|v| v.max(0) as u32),
            output_tokens: output_tokens.max(0) as u32,
            reasoning_tokens: reasoning_tokens.map(|v| v.max(0) as u32),
        };

        let Some(rate) = lookup_rate(pool, &provider_id, &model_id).await? else {
            unknown += 1;
            continue;
        };
        let mut estimate = calculate_cost(&rate, &usage);
        estimate.confidence = format!("backfilled_current_rate:{}", estimate.confidence);
        estimated_total += estimate.amount_usd;
        eligible += 1;

        if !dry_run {
            sqlx::query(
                "UPDATE usage_events
                 SET estimated_cost_usd = ?, cost_confidence = ?, pricing_model_id = ?, cost_formula_json = ?, cost_calculated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(estimate.amount_usd)
            .bind(&estimate.confidence)
            .bind(estimate.pricing_model_id)
            .bind(estimate.formula_json.to_string())
            .bind(id)
            .execute(pool)
            .await?;
        }
    }

    Ok(serde_json::json!({
        "dry_run": dry_run,
        "eligible_rows": eligible,
        "unknown_price_rows": unknown,
        "estimated_total_usd": estimated_total,
    }))
}

pub async fn lookup_rate(
    pool: &SqlitePool,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<PricingRate>, sqlx::Error> {
    let exact = fetch_rate(pool, provider_id, model_id).await?;
    if exact.is_some() {
        return Ok(exact);
    }

    let normalized = normalize_model_id(provider_id, model_id);
    if normalized != model_id {
        return fetch_rate(pool, provider_id, &normalized).await;
    }

    Ok(None)
}

async fn fetch_rate(
    pool: &SqlitePool,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<PricingRate>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String, String, Option<f64>, Option<f64>, Option<f64>, Option<f64>, String, String)>(
        "SELECT id, provider_id, model_id, input_per_1m, cached_input_per_1m, output_per_1m, reasoning_per_1m, confidence, source_kind
         FROM model_pricing
         WHERE provider_id = ? AND model_id = ? AND effective_until IS NULL
         ORDER BY CASE source_kind WHEN 'operator_override' THEN 0 WHEN 'fetched_structured' THEN 1 WHEN 'scraped_html' THEN 2 ELSE 3 END, id DESC
         LIMIT 1",
    )
    .bind(provider_id)
    .bind(model_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(
            |(
                id,
                provider_id,
                model_id,
                input_per_1m,
                cached_input_per_1m,
                output_per_1m,
                reasoning_per_1m,
                confidence,
                source_kind,
            )| PricingRate {
                id: Some(id),
                provider_id,
                model_id,
                input_per_1m,
                cached_input_per_1m,
                output_per_1m,
                reasoning_per_1m,
                confidence,
                source_kind,
            },
        )
    })
}

fn normalize_model_id<'a>(provider_id: &str, model_id: &'a str) -> std::borrow::Cow<'a, str> {
    if provider_id == "deepseek" {
        if model_id == "deepseek-chat" || model_id == "deepseek-reasoner" {
            return std::borrow::Cow::Borrowed(model_id);
        }
        if model_id.contains("v4-pro") {
            return std::borrow::Cow::Borrowed("deepseek-v4-pro");
        }
        if model_id.contains("v4") || model_id.contains("chat") || model_id.contains("reasoner") {
            return std::borrow::Cow::Borrowed("deepseek-v4-flash");
        }
    }

    if provider_id == "xai" || provider_id == "grok" {
        if model_id.contains("grok-3-mini") {
            return std::borrow::Cow::Borrowed("grok-3-mini");
        }
        if model_id.contains("grok-3") {
            return std::borrow::Cow::Borrowed("grok-3");
        }
        if model_id.contains("grok-4") {
            return std::borrow::Cow::Borrowed("grok-4");
        }
    }

    std::borrow::Cow::Borrowed(model_id)
}

pub fn calculate_cost(rate: &PricingRate, usage: &PricingUsage) -> CostEstimate {
    let cached_tokens = usage.cached_input_tokens.unwrap_or(0);
    let miss_tokens = usage.cache_miss_input_tokens.unwrap_or_else(|| {
        usage
            .input_tokens
            .saturating_sub(usage.cached_input_tokens.unwrap_or(0))
    });
    let uncategorized_input = if usage.cache_miss_input_tokens.is_some() {
        0
    } else {
        usage
            .input_tokens
            .saturating_sub(cached_tokens)
            .saturating_sub(miss_tokens)
    };

    let cached_cost = cached_tokens as f64
        * rate
            .cached_input_per_1m
            .or(rate.input_per_1m)
            .unwrap_or(0.0)
        / 1_000_000.0;
    let input_cost =
        (miss_tokens + uncategorized_input) as f64 * rate.input_per_1m.unwrap_or(0.0) / 1_000_000.0;
    let output_cost = usage.output_tokens as f64 * rate.output_per_1m.unwrap_or(0.0) / 1_000_000.0;
    let reasoning_cost = usage.reasoning_tokens.unwrap_or(0) as f64
        * rate.reasoning_per_1m.unwrap_or(0.0)
        / 1_000_000.0;
    let amount_usd = cached_cost + input_cost + output_cost + reasoning_cost;

    CostEstimate {
        amount_usd,
        confidence: rate.confidence.clone(),
        pricing_model_id: rate.id,
        formula_json: serde_json::json!({
            "provider_id": rate.provider_id,
            "pricing_model_id": rate.model_id,
            "source_kind": rate.source_kind,
            "rates_per_1m": {
                "input": rate.input_per_1m,
                "cached_input": rate.cached_input_per_1m,
                "output": rate.output_per_1m,
                "reasoning": rate.reasoning_per_1m,
            },
            "tokens": {
                "input": usage.input_tokens,
                "cached_input": cached_tokens,
                "cache_miss_input": miss_tokens,
                "output": usage.output_tokens,
                "reasoning": usage.reasoning_tokens.unwrap_or(0),
            },
            "components_usd": {
                "input": input_cost,
                "cached_input": cached_cost,
                "output": output_cost,
                "reasoning": reasoning_cost,
            }
        }),
    }
}

pub fn free_tier_estimate() -> CostEstimate {
    CostEstimate {
        amount_usd: 0.0,
        confidence: "free_tier".to_string(),
        pricing_model_id: None,
        formula_json: serde_json::json!({"reason": "free_tier"}),
    }
}

pub fn unknown_price_estimate(
    provider_id: &str,
    model_id: &str,
    usage: &PricingUsage,
) -> CostEstimate {
    CostEstimate {
        amount_usd: 0.0,
        confidence: "unknown_price".to_string(),
        pricing_model_id: None,
        formula_json: serde_json::json!({
            "reason": "unknown_price",
            "provider_id": provider_id,
            "model_id": model_id,
            "tokens": {
                "input": usage.input_tokens,
                "cached_input": usage.cached_input_tokens,
                "cache_miss_input": usage.cache_miss_input_tokens,
                "output": usage.output_tokens,
                "reasoning": usage.reasoning_tokens,
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_cost_uses_cache_detail() {
        let rate = PricingRate {
            id: Some(1),
            provider_id: "deepseek".into(),
            model_id: "deepseek-v4-flash".into(),
            input_per_1m: Some(0.14),
            cached_input_per_1m: Some(0.028),
            output_per_1m: Some(0.28),
            reasoning_per_1m: None,
            confidence: "provider_published".into(),
            source_kind: "builtin".into(),
        };
        let usage = PricingUsage {
            input_tokens: 1_000,
            cached_input_tokens: Some(400),
            cache_miss_input_tokens: Some(600),
            output_tokens: 500,
            reasoning_tokens: None,
        };

        let estimate = calculate_cost(&rate, &usage);
        assert!((estimate.amount_usd - 0.0002352).abs() < 0.0000001);
    }

    #[test]
    fn missing_cache_detail_treats_prompt_as_cache_miss() {
        let rate = PricingRate {
            id: Some(1),
            provider_id: "deepseek".into(),
            model_id: "deepseek-v4-flash".into(),
            input_per_1m: Some(0.14),
            cached_input_per_1m: Some(0.028),
            output_per_1m: Some(0.28),
            reasoning_per_1m: None,
            confidence: "provider_published".into(),
            source_kind: "builtin".into(),
        };
        let usage = PricingUsage {
            input_tokens: 1_000,
            cached_input_tokens: None,
            cache_miss_input_tokens: None,
            output_tokens: 500,
            reasoning_tokens: None,
        };

        let estimate = calculate_cost(&rate, &usage);
        assert!((estimate.amount_usd - 0.00028).abs() < 0.0000001);
    }

    #[test]
    fn deepseek_pricing_fixture_parses() {
        let html = include_str!("../../tests/fixtures/deepseek_pricing.html");
        let rates = parse_deepseek_pricing_html(html).expect("fixture should parse");
        let chat = rates
            .iter()
            .find(|rate| rate.model_id == "deepseek-chat")
            .expect("chat rate");
        assert_eq!(chat.cached_input_per_1m, Some(0.028));
        assert_eq!(chat.input_per_1m, Some(0.14));
        assert_eq!(chat.output_per_1m, Some(0.28));

        let pro = rates
            .iter()
            .find(|rate| rate.model_id == "deepseek-v4-pro")
            .expect("v4 pro rate");
        assert_eq!(pro.input_per_1m, Some(1.74));
        assert_eq!(pro.output_per_1m, Some(3.48));
    }

    #[test]
    fn malformed_pricing_fixture_fails_closed() {
        let err =
            parse_deepseek_pricing_html("<html><table><tr><td>no prices</td></tr></table></html>");
        assert!(err.is_err());
    }
}
