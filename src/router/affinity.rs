//! Process-local subtask affinity.
//!
//! State is intentionally non-durable. A restart drops pins. Multi-replica
//! deployments need session stickiness or a later shared store; SQLite on a
//! network filesystem is not that store.
//!
//! Critical sections do not await. At most one request is in flight for an
//! exact session/subtask scope. Distinct subtasks stay concurrent.

use crate::api::error::ApiError;
use crate::config::schema::{AffinityMode, AgentTier};
use crate::router::continuation::ContinuationSupport;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_TOOL_DIGESTS: usize = 64;
const METADATA_BUDGET: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinOutcome {
    Miss,
    Hit,
    Expired,
    Bypassed { reason: &'static str },
    Established,
    Busy,
    Capacity,
}

impl PinOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Miss => "pin_miss",
            Self::Hit => "pin_hit",
            Self::Expired => "pin_expired",
            Self::Bypassed { reason } => reason,
            Self::Established => "pin_established",
            Self::Busy => "subtask_busy",
            Self::Capacity => "affinity_capacity_exceeded",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PinSnapshot {
    pub provider_id: String,
    pub model_id: String,
    pub tier: AgentTier,
    pub policy_revision: String,
    pub incomplete: bool,
    pub continuation: ContinuationSupport,
    pub generation: u64,
}

struct ScopeEntry {
    generation: u64,
    provider_id: Option<String>,
    model_id: Option<String>,
    tier: Option<AgentTier>,
    policy_revision: String,
    created_ms: u64,
    last_success_ms: Option<u64>,
    absolute_deadline_ms: u64,
    idle_deadline_ms: u64,
    in_flight: bool,
    tool_digests: Vec<String>,
    incomplete: bool,
    continuation: ContinuationSupport,
    project_id: String,
}

pub struct AffinityStore {
    secret: [u8; 32],
    origin: std::time::Instant,
    offset_ms: AtomicU64,
    scopes: DashMap<String, ScopeEntry>,
    admission: Mutex<()>,
    live: AtomicUsize,
    per_project: DashMap<String, usize>,
}

impl AffinityStore {
    pub fn new() -> Self {
        let mut secret = [0u8; 32];
        secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self {
            secret,
            origin: std::time::Instant::now(),
            offset_ms: AtomicU64::new(0),
            scopes: DashMap::new(),
            admission: Mutex::new(()),
            live: AtomicUsize::new(0),
            per_project: DashMap::new(),
        }
    }

    pub fn now_ms(&self) -> u64 {
        (self.origin.elapsed().as_millis() as u64)
            .saturating_add(self.offset_ms.load(Ordering::Relaxed))
    }

    /// Test-only clock advance. Production offset stays zero.
    pub fn advance(&self, duration: Duration) {
        self.offset_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn digest(&self, value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.secret);
        hasher.update([0]);
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn scope_key(
        &self,
        principal_id: &str,
        project_id: &str,
        session: &str,
        subtask: Option<&str>,
        profile_id: &str,
    ) -> String {
        let raw = format!(
            "{principal_id}\n{project_id}\n{session}\n{}\n{profile_id}",
            subtask.unwrap_or("default")
        );
        self.digest(&raw)
    }

    pub fn live_scopes(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self, key: &str) -> Option<PinSnapshot> {
        let now = self.now_ms();
        let entry = self.scopes.get(key)?;
        if entry.absolute_deadline_ms <= now {
            return None;
        }
        if entry
            .last_success_ms
            .is_some_and(|_| entry.idle_deadline_ms <= now)
        {
            return None;
        }
        Some(PinSnapshot {
            provider_id: entry.provider_id.clone()?,
            model_id: entry.model_id.clone()?,
            tier: entry.tier?,
            policy_revision: entry.policy_revision.clone(),
            incomplete: entry.incomplete,
            continuation: entry.continuation,
            generation: entry.generation,
        })
    }

    pub fn try_admit(
        self: &Arc<Self>,
        key: String,
        project_id: &str,
        max_sessions: usize,
        max_per_project: usize,
        idle_ttl: Duration,
        max_lifetime: Duration,
    ) -> Result<AffinityLease, ApiError> {
        let _guard = self.admission.lock().unwrap_or_else(|err| err.into_inner());
        let now = self.now_ms();
        if let Some(mut entry) = self.scopes.get_mut(&key) {
            if entry.absolute_deadline_ms <= now
                || entry
                    .last_success_ms
                    .is_some_and(|_| entry.idle_deadline_ms <= now)
            {
                entry.provider_id = None;
                entry.model_id = None;
                entry.tier = None;
                entry.last_success_ms = None;
                entry.tool_digests.clear();
                entry.incomplete = false;
                entry.created_ms = now;
                entry.absolute_deadline_ms = now.saturating_add(max_lifetime.as_millis() as u64);
                entry.generation = entry.generation.saturating_add(1);
            }
            if entry.in_flight {
                return Err(agent_error(
                    409,
                    "subtask_busy",
                    "this session subtask already has a request in flight",
                    Some(1),
                ));
            }
            entry.in_flight = true;
            entry.idle_deadline_ms = now.saturating_add(idle_ttl.as_millis() as u64);
            let generation = entry.generation;
            drop(entry);
            return Ok(AffinityLease {
                store: Arc::clone(self),
                key,
                generation,
                released: AtomicBool::new(false),
            });
        }

        let project_live = self
            .per_project
            .get(project_id)
            .map(|count| *count)
            .unwrap_or(0);
        if self.live.load(Ordering::Relaxed) >= max_sessions || project_live >= max_per_project {
            return Err(agent_error(
                429,
                "affinity_capacity_exceeded",
                "affinity scope capacity is exhausted; retry without affinity or later",
                Some(5),
            ));
        }

        self.scopes.insert(
            key.clone(),
            ScopeEntry {
                generation: 1,
                provider_id: None,
                model_id: None,
                tier: None,
                policy_revision: String::new(),
                created_ms: now,
                last_success_ms: None,
                absolute_deadline_ms: now.saturating_add(max_lifetime.as_millis() as u64),
                idle_deadline_ms: now.saturating_add(idle_ttl.as_millis() as u64),
                in_flight: true,
                tool_digests: Vec::new(),
                incomplete: false,
                continuation: ContinuationSupport::Replayable,
                project_id: project_id.to_string(),
            },
        );
        self.live.fetch_add(1, Ordering::Relaxed);
        *self.per_project.entry(project_id.to_string()).or_insert(0) += 1;
        Ok(AffinityLease {
            store: Arc::clone(self),
            key,
            generation: 1,
            released: AtomicBool::new(false),
        })
    }

    pub fn sweep(&self) {
        let now = self.now_ms();
        let expired: Vec<String> = self
            .scopes
            .iter()
            .filter(|entry| {
                !entry.in_flight
                    && (entry.absolute_deadline_ms <= now
                        || entry.last_success_ms.is_some_and(|_| {
                            entry.idle_deadline_ms <= now && entry.provider_id.is_some()
                        })
                        || (entry.provider_id.is_none() && entry.idle_deadline_ms <= now))
            })
            .map(|entry| entry.key().clone())
            .collect();
        let _guard = self.admission.lock().unwrap_or_else(|err| err.into_inner());
        for key in expired {
            if let Some((_, entry)) = self.scopes.remove(&key) {
                if entry.in_flight {
                    self.scopes.insert(key, entry);
                    continue;
                }
                self.live.fetch_sub(1, Ordering::Relaxed);
                if let Some(mut count) = self.per_project.get_mut(&entry.project_id) {
                    *count = count.saturating_sub(1);
                }
            }
        }
    }

    fn release_inflight(&self, key: &str, generation: u64) {
        if let Some(mut entry) = self.scopes.get_mut(key) {
            if entry.generation == generation {
                entry.in_flight = false;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn commit(
        &self,
        key: &str,
        generation: u64,
        provider_id: &str,
        model_id: &str,
        tier: AgentTier,
        policy_revision: &str,
        tool_ids: &[String],
        continuation: ContinuationSupport,
        incomplete: bool,
        idle_ttl: Duration,
    ) {
        let now = self.now_ms();
        let mut digests = Vec::new();
        let mut bytes = 0usize;
        for id in tool_ids.iter().take(MAX_TOOL_DIGESTS) {
            let digest = self.digest(id);
            bytes = bytes.saturating_add(digest.len());
            if bytes > METADATA_BUDGET {
                break;
            }
            digests.push(digest);
        }
        if let Some(mut entry) = self.scopes.get_mut(key) {
            if entry.generation != generation {
                return;
            }
            entry.provider_id = Some(provider_id.to_string());
            entry.model_id = Some(model_id.to_string());
            entry.tier = Some(tier);
            entry.policy_revision = policy_revision.to_string();
            entry.last_success_ms = Some(now);
            entry.idle_deadline_ms = now.saturating_add(idle_ttl.as_millis() as u64);
            entry.tool_digests = digests;
            entry.incomplete = incomplete || tool_ids.len() > MAX_TOOL_DIGESTS;
            entry.continuation = continuation;
            entry.in_flight = false;
        }
    }

    fn mark_incomplete(&self, key: &str, generation: u64) {
        if let Some(mut entry) = self.scopes.get_mut(key) {
            if entry.generation == generation {
                entry.incomplete = true;
                entry.in_flight = false;
            }
        }
    }
}

impl Default for AffinityStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AffinityLease {
    store: Arc<AffinityStore>,
    key: String,
    generation: u64,
    released: AtomicBool,
}

impl AffinityLease {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_success(
        &self,
        provider_id: &str,
        model_id: &str,
        tier: AgentTier,
        policy_revision: &str,
        tool_ids: &[String],
        continuation: ContinuationSupport,
        idle_ttl: Duration,
    ) {
        self.store.commit(
            &self.key,
            self.generation,
            provider_id,
            model_id,
            tier,
            policy_revision,
            tool_ids,
            continuation,
            false,
            idle_ttl,
        );
        self.released.store(true, Ordering::Relaxed);
    }

    pub fn mark_incomplete(&self) {
        self.store.mark_incomplete(&self.key, self.generation);
        self.released.store(true, Ordering::Relaxed);
    }
}

impl Drop for AffinityLease {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::Relaxed) {
            self.store.release_inflight(&self.key, self.generation);
        }
    }
}

/// Streaming pin guard. Drop commits a completed stream, marks an incomplete
/// continuation after visible output, or releases the lease if nothing was sent.
pub struct StreamPin {
    lease: AffinityLease,
    pub visible: bool,
    pub completed: bool,
    pub provider_id: String,
    pub model_id: String,
    pub tier: AgentTier,
    pub policy_revision: String,
    pub tool_ids: Vec<String>,
    pub continuation: ContinuationSupport,
    pub idle_ttl: Duration,
}

impl StreamPin {
    pub fn new(
        lease: AffinityLease,
        tier: AgentTier,
        policy_revision: String,
        idle_ttl: Duration,
    ) -> Self {
        Self {
            lease,
            visible: false,
            completed: false,
            provider_id: String::new(),
            model_id: String::new(),
            tier,
            policy_revision,
            tool_ids: Vec::new(),
            continuation: ContinuationSupport::Replayable,
            idle_ttl,
        }
    }

    pub fn observe(&mut self, provider_id: &str, model_id: &str, tool_id: Option<&str>) {
        self.visible = true;
        self.provider_id = provider_id.to_string();
        self.model_id = model_id.to_string();
        self.continuation = crate::router::continuation::continuation_support(provider_id);
        if let Some(tool_id) = tool_id {
            if !tool_id.is_empty()
                && self.tool_ids.len() < MAX_TOOL_DIGESTS
                && !self.tool_ids.iter().any(|existing| existing == tool_id)
            {
                self.tool_ids.push(tool_id.to_string());
            }
        }
    }
}

impl Drop for StreamPin {
    fn drop(&mut self) {
        if self.completed && !self.provider_id.is_empty() {
            self.lease.commit_success(
                &self.provider_id,
                &self.model_id,
                self.tier,
                &self.policy_revision,
                &self.tool_ids,
                self.continuation,
                self.idle_ttl,
            );
        } else if self.visible {
            self.lease.mark_incomplete();
        }
    }
}

pub fn effective_affinity(
    operator: AffinityMode,
    requested: Option<AffinityHintAlias>,
    force_required: bool,
) -> AffinityMode {
    if force_required {
        return AffinityMode::Required;
    }
    match operator {
        AffinityMode::Off => AffinityMode::Off,
        AffinityMode::Required => AffinityMode::Required,
        AffinityMode::Prefer => match requested {
            Some(AffinityHintAlias::Off) => AffinityMode::Off,
            Some(AffinityHintAlias::Required) => AffinityMode::Required,
            _ => AffinityMode::Prefer,
        },
    }
}

#[derive(Clone, Copy)]
pub enum AffinityHintAlias {
    Off,
    Prefer,
    Required,
}

fn agent_error(
    status: u16,
    code: &'static str,
    message: &str,
    retry_after: Option<u64>,
) -> ApiError {
    ApiError::AgentRouting {
        http_status: status,
        code,
        message: message.to_string(),
        retry_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Arc<AffinityStore> {
        Arc::new(AffinityStore::new())
    }

    #[test]
    fn same_scope_second_request_is_busy_and_siblings_are_not() {
        let store = store();
        let first = store
            .try_admit(
                "scope-a".into(),
                "default",
                10,
                10,
                Duration::from_secs(600),
                Duration::from_secs(3600),
            )
            .unwrap();
        let busy = store.try_admit(
            "scope-a".into(),
            "default",
            10,
            10,
            Duration::from_secs(600),
            Duration::from_secs(3600),
        );
        assert!(matches!(
            busy,
            Err(ApiError::AgentRouting {
                code: "subtask_busy",
                ..
            })
        ));
        let sibling = store
            .try_admit(
                "scope-b".into(),
                "default",
                10,
                10,
                Duration::from_secs(600),
                Duration::from_secs(3600),
            )
            .unwrap();
        drop(sibling);
        drop(first);
        let again = store.try_admit(
            "scope-a".into(),
            "default",
            10,
            10,
            Duration::from_secs(600),
            Duration::from_secs(3600),
        );
        assert!(again.is_ok());
    }

    #[test]
    fn expiry_and_generation_ignore_late_commits() {
        let store = store();
        let lease = store
            .try_admit(
                "scope".into(),
                "proj",
                10,
                10,
                Duration::from_secs(10),
                Duration::from_secs(60),
            )
            .unwrap();
        lease.commit_success(
            "groq",
            "cheap",
            AgentTier::Economy,
            "rev",
            &[],
            ContinuationSupport::Replayable,
            Duration::from_secs(10),
        );
        assert!(store.snapshot("scope").is_some());
        store.advance(Duration::from_secs(11));
        assert!(store.snapshot("scope").is_none());

        let lease = store
            .try_admit(
                "scope".into(),
                "proj",
                10,
                10,
                Duration::from_secs(10),
                Duration::from_secs(60),
            )
            .unwrap();
        let generation = lease.generation();
        drop(lease);
        store.commit(
            "scope",
            generation.saturating_sub(1),
            "groq",
            "stale",
            AgentTier::Advanced,
            "rev",
            &[],
            ContinuationSupport::Replayable,
            false,
            Duration::from_secs(10),
        );
        let snap = store.snapshot("scope");
        assert!(snap.is_none() || snap.unwrap().model_id != "stale");
    }

    #[test]
    fn capacity_rejects_new_scopes_without_evicting_active_leases() {
        let store = store();
        let lease = store
            .try_admit(
                "active".into(),
                "proj",
                1,
                1,
                Duration::from_secs(10),
                Duration::from_secs(60),
            )
            .unwrap();
        let rejected = store.try_admit(
            "other".into(),
            "proj",
            1,
            1,
            Duration::from_secs(10),
            Duration::from_secs(60),
        );
        assert!(matches!(
            rejected,
            Err(ApiError::AgentRouting {
                code: "affinity_capacity_exceeded",
                ..
            })
        ));
        assert!(store.snapshot("active").is_none());
        assert_eq!(store.live_scopes(), 1);
        drop(lease);
    }

    #[test]
    #[ignore = "30-minute affinity churn; run explicitly and record RSS"]
    fn thirty_minute_churn_keeps_affinity_memory_bounded() {
        let store = store();
        let started = std::time::Instant::now();
        let mut max_live = 0usize;
        let mut max_rss_kb = 0u64;
        let mut cycles = 0u64;
        let mut admitted = 0u64;
        let mut sequence = 0u64;
        let start_rss_kb = process_rss_kb().unwrap_or(0);
        while started.elapsed() < std::time::Duration::from_secs(30 * 60) {
            for _ in 0..200 {
                sequence += 1;
                let key = format!("churn-{sequence}");
                let Ok(lease) = store.try_admit(
                    key,
                    "proj",
                    10_000,
                    10_000,
                    Duration::from_secs(1),
                    Duration::from_secs(2),
                ) else {
                    continue;
                };
                admitted += 1;
                if sequence % 2 == 0 {
                    lease.commit_success(
                        "groq",
                        "cheap",
                        AgentTier::Economy,
                        "rev",
                        &[],
                        ContinuationSupport::Replayable,
                        Duration::from_secs(1),
                    );
                }
                drop(lease);
            }
            store.advance(Duration::from_secs(3));
            store.sweep();
            max_live = max_live.max(store.live_scopes());
            if let Some(rss) = process_rss_kb() {
                max_rss_kb = max_rss_kb.max(rss);
            }
            cycles += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        let summary = format!(
            "cycles={cycles} admitted={admitted} max_live={} final_live={} start_rss_kb={start_rss_kb} max_rss_kb={max_rss_kb}\n",
            max_live,
            store.live_scopes()
        );
        let _ = std::fs::write("/tmp/tokenscavenger-affinity-soak.txt", &summary);
        assert!(store.live_scopes() <= 10_000, "{summary}");
        assert!(max_live <= 10_000, "{summary}");
        assert!(admitted > 10_000, "{summary}");
    }

    fn process_rss_kb() -> Option<u64> {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        String::from_utf8(output.stdout)
            .ok()?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }
}
