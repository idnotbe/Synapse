//! Advisory target ownership claims for multi-agent coordination (#797).
//!
//! Claims are daemon-local read-model state keyed by target identity. They do
//! not replace per-tool validation or capability checks; they make same-target
//! mutation conflicts explicit before another session silently clobbers a
//! claimed window/tab.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use rmcp::{RoleServer, model::ErrorCode, service::RequestContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use synapse_core::error_codes;

use super::{
    ErrorData, Json, Parameters, SessionTarget, SynapseService, TargetWire, mcp_error,
    session_registry::unix_time_ms_now, tool, tool_router,
};

const DEFAULT_TARGET_CLAIM_TTL_MS: u64 = 120_000;
const MIN_TARGET_CLAIM_TTL_MS: u64 = 1_000;
const MAX_TARGET_CLAIM_TTL_MS: u64 = 600_000;

pub(crate) type SharedTargetClaims = Arc<Mutex<TargetClaimRegistry>>;

#[derive(Debug, Default)]
pub(crate) struct TargetClaimRegistry {
    claims: BTreeMap<String, TargetClaimEntry>,
}

#[derive(Clone, Debug)]
pub(crate) struct TargetClaimEntry {
    target_key: String,
    target: SessionTarget,
    owner_session_id: String,
    claimed_at_unix_ms: u64,
    renewed_at_unix_ms: u64,
    ttl_ms: u64,
    expires_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetClaimTargetParam {
    Window {
        window_hwnd: i64,
    },
    Cdp {
        window_hwnd: i64,
        cdp_target_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimParams {
    #[serde(default)]
    #[schemars(default)]
    pub target: Option<TargetClaimTargetParam>,
    #[serde(default = "default_target_claim_ttl_ms")]
    #[schemars(
        default = "default_target_claim_ttl_ms",
        range(min = 1000, max = 600000)
    )]
    pub ttl_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetReleaseParams {
    #[serde(default)]
    #[schemars(default)]
    pub target: Option<TargetClaimTargetParam>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimStatusParams {
    #[serde(default)]
    #[schemars(default)]
    pub target: Option<TargetClaimTargetParam>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimRead {
    pub target_key: String,
    pub target: TargetWire,
    pub owner_session_id: String,
    pub claimed_at_unix_ms: u64,
    pub renewed_at_unix_ms: u64,
    pub ttl_ms: u64,
    pub expires_at_unix_ms: u64,
    pub expires_in_ms: u64,
    pub source_of_truth: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimResponse {
    pub session_id: String,
    pub outcome: String,
    pub claim: TargetClaimRead,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetReleaseResponse {
    pub session_id: String,
    pub target_key: String,
    pub target: TargetWire,
    pub released: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_claim: Option<TargetClaimRead>,
    pub source_of_truth: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimStatusResponse {
    pub session_id: String,
    pub now_unix_ms: u64,
    pub claim_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_claim: Option<TargetClaimRead>,
    pub claims: Vec<TargetClaimRead>,
    pub source_of_truth: String,
}

#[derive(Clone, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetClaimCleanupReport {
    pub owned_before: usize,
    pub released: usize,
    pub target_keys: Vec<String>,
    pub failed: bool,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct TargetClaimConflict {
    entry: TargetClaimEntry,
    requester_session_id: String,
    tool: &'static str,
}

#[tool_router(router = target_claim_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "Claim this MCP session's active target, or an explicit window/CDP target, as an advisory ownership lease. A live claim causes other sessions' mutating actions against the same target to fail closed with TARGET_CO_OWNED while read-only observe remains allowed."
    )]
    pub async fn target_claim(
        &self,
        params: Parameters<TargetClaimParams>,
        request_context: RequestContext<RoleServer>,
    ) -> Result<Json<TargetClaimResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = "target_claim",
            "tool.invocation kind=target_claim"
        );
        let session_id = require_claim_session_id(&request_context)?;
        validate_target_claim_ttl(params.0.ttl_ms)?;
        let target = self.resolve_target_claim_target(&session_id, params.0.target)?;
        validate_claim_target(&target)?;
        let now = unix_time_ms_now();
        let live_sessions = self.live_target_claim_sessions(now, Some(&session_id))?;
        let mut guard = self.lock_target_claims()?;
        guard.prune_inactive(now, &live_sessions);
        match guard.claim(&session_id, target, params.0.ttl_ms, now, &live_sessions) {
            Ok((entry, outcome)) => {
                tracing::info!(
                    code = "TARGET_CLAIM_SET",
                    session_id = %session_id,
                    target_key = %entry.target_key,
                    outcome,
                    expires_at_unix_ms = entry.expires_at_unix_ms,
                    "readback=target_claim outcome={outcome}"
                );
                Ok(Json(TargetClaimResponse {
                    session_id,
                    outcome: outcome.to_owned(),
                    claim: entry.read(now),
                }))
            }
            Err(conflict) => Err(conflict_error(
                "target_claim",
                &session_id,
                &conflict,
                "target_claim",
            )),
        }
    }

    #[tool(
        description = "Release this MCP session's advisory target ownership claim for the active target, or an explicit window/CDP target. A session cannot release another session's claim."
    )]
    pub async fn target_release(
        &self,
        params: Parameters<TargetReleaseParams>,
        request_context: RequestContext<RoleServer>,
    ) -> Result<Json<TargetReleaseResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = "target_release",
            "tool.invocation kind=target_release"
        );
        let session_id = require_claim_session_id(&request_context)?;
        let target = self.resolve_target_claim_target(&session_id, params.0.target)?;
        validate_claim_target(&target)?;
        let target_key = target_key(&target);
        let now = unix_time_ms_now();
        let live_sessions = self.live_target_claim_sessions(now, Some(&session_id))?;
        let mut guard = self.lock_target_claims()?;
        guard.prune_inactive(now, &live_sessions);
        match guard.release(&session_id, &target_key) {
            Ok(released) => {
                let released_claim = released.as_ref().map(|entry| entry.read(now));
                tracing::info!(
                    code = "TARGET_CLAIM_RELEASED",
                    session_id = %session_id,
                    target_key = %target_key,
                    released = released.is_some(),
                    "readback=target_release"
                );
                Ok(Json(TargetReleaseResponse {
                    session_id,
                    target_key,
                    target: target_wire(&target),
                    released: released.is_some(),
                    released_claim,
                    source_of_truth: source_of_truth(),
                }))
            }
            Err(conflict) => Err(conflict_error(
                "target_release",
                &session_id,
                &conflict,
                "target_release",
            )),
        }
    }

    #[tool(
        description = "Read the live advisory target ownership claims. With target omitted, returns all live claims; with target supplied or active, also returns the claim for that target."
    )]
    pub async fn target_claim_status(
        &self,
        params: Parameters<TargetClaimStatusParams>,
        request_context: RequestContext<RoleServer>,
    ) -> Result<Json<TargetClaimStatusResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = "target_claim_status",
            "tool.invocation kind=target_claim_status"
        );
        let session_id = require_claim_session_id(&request_context)?;
        let target = match params.0.target {
            Some(target) => Some(target_param_to_session_target(target)?),
            None => self.session_target(Some(&session_id))?,
        };
        if let Some(target) = &target {
            validate_claim_target(target)?;
        }
        let now = unix_time_ms_now();
        let live_sessions = self.live_target_claim_sessions(now, Some(&session_id))?;
        let mut guard = self.lock_target_claims()?;
        guard.prune_inactive(now, &live_sessions);
        let target_key = target.as_ref().map(target_key);
        let claims = guard.reads(now);
        let target_claim = target_key
            .as_ref()
            .and_then(|key| claims.iter().find(|claim| &claim.target_key == key))
            .cloned();
        Ok(Json(TargetClaimStatusResponse {
            session_id,
            now_unix_ms: now,
            claim_count: claims.len(),
            target_key,
            target_claim,
            claims,
            source_of_truth: source_of_truth(),
        }))
    }
}

impl SynapseService {
    pub(crate) fn lock_target_claims(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, TargetClaimRegistry>, ErrorData> {
        self.target_claims.lock().map_err(|_error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "target claim registry lock poisoned",
            )
        })
    }

    pub(crate) fn target_claim_reads_by_owner(
        &self,
    ) -> Result<BTreeMap<String, Vec<TargetClaimRead>>, ErrorData> {
        let now = unix_time_ms_now();
        let live_sessions = self.live_target_claim_sessions(now, None)?;
        let mut guard = self.lock_target_claims()?;
        guard.prune_inactive(now, &live_sessions);
        let mut by_owner: BTreeMap<String, Vec<TargetClaimRead>> = BTreeMap::new();
        for claim in guard.reads(now) {
            by_owner
                .entry(claim.owner_session_id.clone())
                .or_default()
                .push(claim);
        }
        Ok(by_owner)
    }

    pub(crate) fn ensure_target_claim_allows_action(
        &self,
        tool: &'static str,
        explicit_target: Option<SessionTarget>,
        request_context: &RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let Some(session_id) =
            super::context::mcp_session_id_from_request_context(request_context)?
        else {
            return Ok(());
        };
        let target = match explicit_target {
            Some(target) => Some(target),
            None => self.session_target(Some(&session_id))?,
        };
        let target = match target {
            Some(target) => target,
            None => current_foreground_session_target()?,
        };
        self.ensure_target_claim_allows_session(tool, &session_id, &target)
    }

    pub(crate) fn ensure_target_claim_allows_session(
        &self,
        tool: &'static str,
        session_id: &str,
        target: &SessionTarget,
    ) -> Result<(), ErrorData> {
        let now = unix_time_ms_now();
        let live_sessions = self.live_target_claim_sessions(now, Some(session_id))?;
        let mut guard = self.lock_target_claims()?;
        guard.prune_inactive(now, &live_sessions);
        if let Some(entry) = guard.conflict(session_id, target) {
            return Err(conflict_error(
                tool,
                session_id,
                &TargetClaimConflict {
                    entry,
                    requester_session_id: session_id.to_owned(),
                    tool,
                },
                "mutating_action_claim_check",
            ));
        }
        Ok(())
    }

    fn resolve_target_claim_target(
        &self,
        session_id: &str,
        target: Option<TargetClaimTargetParam>,
    ) -> Result<SessionTarget, ErrorData> {
        match target {
            Some(target) => target_param_to_session_target(target),
            None => self.session_target(Some(session_id))?.ok_or_else(|| {
                mcp_error(
                    error_codes::TARGET_NOT_SET,
                    "target_claim requires an explicit target or this session's active target",
                )
            }),
        }
    }

    fn live_target_claim_sessions(
        &self,
        now_unix_ms: u64,
        include_session_id: Option<&str>,
    ) -> Result<BTreeSet<String>, ErrorData> {
        let mut live = BTreeSet::new();
        if let Some(session_id) = include_session_id {
            live.insert(session_id.to_owned());
        }
        let guard = self.session_registry_ref().lock().map_err(|_error| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                "session registry lock poisoned while reading target claim owner liveness",
            )
        })?;
        for read in guard.reads(now_unix_ms) {
            if read.lifecycle == "live" {
                live.insert(read.session_id);
            }
        }
        Ok(live)
    }
}

pub(crate) fn cleanup_claims_for_session(
    claims: &SharedTargetClaims,
    session_id: &str,
) -> TargetClaimCleanupReport {
    match claims.lock() {
        Ok(mut claims) => {
            let owned_before = claims
                .claims
                .values()
                .filter(|entry| entry.owner_session_id == session_id)
                .count();
            let released = claims.release_owner(session_id);
            TargetClaimCleanupReport {
                owned_before,
                released: released.len(),
                target_keys: released
                    .iter()
                    .map(|entry| entry.target_key.clone())
                    .collect(),
                failed: false,
                error_message: None,
            }
        }
        Err(_error) => TargetClaimCleanupReport {
            failed: true,
            error_message: Some("target claim registry lock poisoned".to_owned()),
            ..TargetClaimCleanupReport::default()
        },
    }
}

impl TargetClaimRegistry {
    fn claim(
        &mut self,
        session_id: &str,
        target: SessionTarget,
        ttl_ms: u64,
        now_unix_ms: u64,
        live_sessions: &BTreeSet<String>,
    ) -> Result<(TargetClaimEntry, &'static str), TargetClaimConflict> {
        let target_key = target_key(&target);
        if let Some(existing) = self.claims.get(&target_key)
            && existing.owner_session_id != session_id
            && live_sessions.contains(&existing.owner_session_id)
        {
            return Err(TargetClaimConflict {
                entry: existing.clone(),
                requester_session_id: session_id.to_owned(),
                tool: "target_claim",
            });
        }
        let outcome = if self.claims.contains_key(&target_key) {
            "renewed"
        } else {
            "claimed"
        };
        let entry = self
            .claims
            .entry(target_key.clone())
            .and_modify(|entry| {
                entry.owner_session_id = session_id.to_owned();
                entry.renewed_at_unix_ms = now_unix_ms;
                entry.ttl_ms = ttl_ms;
                entry.expires_at_unix_ms = now_unix_ms.saturating_add(ttl_ms);
            })
            .or_insert_with(|| TargetClaimEntry {
                target_key,
                target,
                owner_session_id: session_id.to_owned(),
                claimed_at_unix_ms: now_unix_ms,
                renewed_at_unix_ms: now_unix_ms,
                ttl_ms,
                expires_at_unix_ms: now_unix_ms.saturating_add(ttl_ms),
            })
            .clone();
        Ok((entry, outcome))
    }

    fn release(
        &mut self,
        session_id: &str,
        target_key: &str,
    ) -> Result<Option<TargetClaimEntry>, TargetClaimConflict> {
        let Some(existing) = self.claims.get(target_key).cloned() else {
            return Ok(None);
        };
        if existing.owner_session_id != session_id {
            return Err(TargetClaimConflict {
                entry: existing,
                requester_session_id: session_id.to_owned(),
                tool: "target_release",
            });
        }
        Ok(self.claims.remove(target_key))
    }

    fn release_owner(&mut self, session_id: &str) -> Vec<TargetClaimEntry> {
        let target_keys = self
            .claims
            .iter()
            .filter_map(|(key, entry)| (entry.owner_session_id == session_id).then(|| key.clone()))
            .collect::<Vec<_>>();
        target_keys
            .into_iter()
            .filter_map(|key| self.claims.remove(&key))
            .collect()
    }

    fn conflict(&self, session_id: &str, target: &SessionTarget) -> Option<TargetClaimEntry> {
        self.claims
            .get(&target_key(target))
            .filter(|entry| entry.owner_session_id != session_id)
            .cloned()
    }

    pub(crate) fn reads(&self, now_unix_ms: u64) -> Vec<TargetClaimRead> {
        self.claims
            .values()
            .map(|entry| entry.read(now_unix_ms))
            .collect()
    }

    fn prune_inactive(&mut self, now_unix_ms: u64, live_sessions: &BTreeSet<String>) {
        self.claims.retain(|_key, entry| {
            entry.expires_at_unix_ms > now_unix_ms
                && live_sessions.contains(&entry.owner_session_id)
        });
    }
}

impl TargetClaimEntry {
    fn read(&self, now_unix_ms: u64) -> TargetClaimRead {
        TargetClaimRead {
            target_key: self.target_key.clone(),
            target: target_wire(&self.target),
            owner_session_id: self.owner_session_id.clone(),
            claimed_at_unix_ms: self.claimed_at_unix_ms,
            renewed_at_unix_ms: self.renewed_at_unix_ms,
            ttl_ms: self.ttl_ms,
            expires_at_unix_ms: self.expires_at_unix_ms,
            expires_in_ms: self.expires_at_unix_ms.saturating_sub(now_unix_ms),
            source_of_truth: source_of_truth(),
        }
    }
}

pub(crate) fn target_key(target: &SessionTarget) -> String {
    match target {
        SessionTarget::Window { hwnd } => format!("window:0x{hwnd:x}"),
        SessionTarget::Cdp {
            window_hwnd,
            cdp_target_id,
        } => format!("cdp:0x{window_hwnd:x}:{cdp_target_id}"),
    }
}

pub(crate) fn target_wire(target: &SessionTarget) -> TargetWire {
    match target {
        SessionTarget::Window { hwnd } => TargetWire::Window { window_hwnd: *hwnd },
        SessionTarget::Cdp {
            window_hwnd,
            cdp_target_id,
        } => TargetWire::Cdp {
            window_hwnd: *window_hwnd,
            cdp_target_id: cdp_target_id.clone(),
        },
    }
}

pub(crate) fn window_session_target(hwnd: i64) -> SessionTarget {
    SessionTarget::Window { hwnd }
}

fn current_foreground_session_target() -> Result<SessionTarget, ErrorData> {
    let foreground = synapse_a11y::current_foreground_context().map_err(|error| {
        mcp_error(
            error_codes::TARGET_WINDOW_NOT_FOUND,
            format!("target claim foreground read failed before mutating action: {error}"),
        )
    })?;
    validate_claim_target(&SessionTarget::Window {
        hwnd: foreground.hwnd,
    })?;
    Ok(SessionTarget::Window {
        hwnd: foreground.hwnd,
    })
}

fn target_param_to_session_target(
    target: TargetClaimTargetParam,
) -> Result<SessionTarget, ErrorData> {
    match target {
        TargetClaimTargetParam::Window { window_hwnd } => {
            Ok(SessionTarget::Window { hwnd: window_hwnd })
        }
        TargetClaimTargetParam::Cdp {
            window_hwnd,
            cdp_target_id,
        } => {
            validate_cdp_target_id(&cdp_target_id)?;
            Ok(SessionTarget::Cdp {
                window_hwnd,
                cdp_target_id,
            })
        }
    }
}

fn validate_claim_target(target: &SessionTarget) -> Result<(), ErrorData> {
    match target {
        SessionTarget::Window { hwnd }
        | SessionTarget::Cdp {
            window_hwnd: hwnd, ..
        } => {
            if *hwnd == 0 {
                return Err(mcp_error(
                    error_codes::TOOL_PARAMS_INVALID,
                    "target claim window_hwnd must be non-zero",
                ));
            }
            synapse_a11y::foreground_context(*hwnd).map_err(|error| {
                mcp_error(
                    error_codes::TARGET_WINDOW_NOT_FOUND,
                    format!("target claim window_hwnd 0x{hwnd:x} is not live: {error}"),
                )
            })?;
        }
    }
    Ok(())
}

fn validate_cdp_target_id(cdp_target_id: &str) -> Result<(), ErrorData> {
    if cdp_target_id.trim().is_empty() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            "target claim cdp_target_id must not be empty",
        ));
    }
    if cdp_target_id.chars().count() > 256 {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            "target claim cdp_target_id must be at most 256 Unicode scalar values",
        ));
    }
    if !cdp_target_id.chars().all(|ch| ('!'..='~').contains(&ch)) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            "target claim cdp_target_id must contain only visible ASCII characters",
        ));
    }
    Ok(())
}

fn validate_target_claim_ttl(ttl_ms: u64) -> Result<(), ErrorData> {
    if !(MIN_TARGET_CLAIM_TTL_MS..=MAX_TARGET_CLAIM_TTL_MS).contains(&ttl_ms) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "target_claim ttl_ms must be in {MIN_TARGET_CLAIM_TTL_MS}..={MAX_TARGET_CLAIM_TTL_MS}, got {ttl_ms}"
            ),
        ));
    }
    Ok(())
}

fn conflict_error(
    tool: &'static str,
    requester_session_id: &str,
    conflict: &TargetClaimConflict,
    operation: &'static str,
) -> ErrorData {
    let now = unix_time_ms_now();
    let holder = conflict.entry.owner_session_id.clone();
    let read = conflict.entry.read(now);
    tracing::warn!(
        code = error_codes::TARGET_CO_OWNED,
        tool,
        operation,
        requester_session_id,
        holder_session_id = %holder,
        target_key = %conflict.entry.target_key,
        conflict_tool = conflict.tool,
        conflict_requester_session_id = %conflict.requester_session_id,
        "target claim conflict"
    );
    ErrorData::new(
        ErrorCode(-32099),
        format!(
            "{tool} refused target {} because it is claimed by live MCP session {holder}",
            conflict.entry.target_key
        ),
        Some(json!({
            "code": error_codes::TARGET_CO_OWNED,
            "tool": tool,
            "operation": operation,
            "source_of_truth": source_of_truth(),
            "target_key": conflict.entry.target_key,
            "target": read.target,
            "requester_session_id": requester_session_id,
            "holder_session_id": holder,
            "claim": read,
            "read_only_observe_allowed": true,
            "mutation_allowed": false,
            "resolution": "release the claim, wait for claim expiry, or use a different target",
        })),
    )
}

fn require_claim_session_id(
    request_context: &RequestContext<RoleServer>,
) -> Result<String, ErrorData> {
    super::context::mcp_session_id_from_request_context(request_context)?.ok_or_else(|| {
        mcp_error(
            error_codes::HTTP_SESSION_INVALID,
            "target claim tools require an MCP session id",
        )
    })
}

const fn default_target_claim_ttl_ms() -> u64 {
    DEFAULT_TARGET_CLAIM_TTL_MS
}

fn source_of_truth() -> String {
    "daemon target claim registry".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_conflict_is_reported_for_live_other_owner() {
        let mut registry = TargetClaimRegistry::default();
        let target = SessionTarget::Window { hwnd: 0x1234 };
        let live = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
        let first = registry
            .claim("a", target.clone(), 10_000, 1_000, &live)
            .expect("first claim should succeed");
        println!(
            "readback=target_claim first key={} owner={}",
            first.0.target_key, first.0.owner_session_id
        );

        let conflict = registry
            .claim("b", target, 10_000, 1_001, &live)
            .expect_err("second live owner must conflict");

        assert_eq!(conflict.entry.owner_session_id, "a");
        assert_eq!(conflict.entry.target_key, "window:0x1234");
    }

    #[test]
    fn expired_or_stale_claim_does_not_block_new_owner() {
        let mut registry = TargetClaimRegistry::default();
        let target = SessionTarget::Window { hwnd: 0x5678 };
        let live_a = BTreeSet::from(["a".to_owned()]);
        registry
            .claim("a", target.clone(), 1_000, 1_000, &live_a)
            .expect("initial claim should succeed");
        let live_b = BTreeSet::from(["b".to_owned()]);
        registry.prune_inactive(2_001, &live_b);
        let second = registry
            .claim("b", target, 10_000, 2_001, &live_b)
            .expect("expired stale claim should be pruned");

        println!(
            "readback=target_claim edge=expired owner_after={}",
            second.0.owner_session_id
        );
        assert_eq!(second.0.owner_session_id, "b");
    }
}
