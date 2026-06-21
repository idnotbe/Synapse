//! Network capture listing tools (#1081) backed by the a11y CDP Network buffer.

use super::{
    ErrorData, Json, Parameters, SynapseService,
    m1_tools::{
        browser_raw_cdp_required_error, cdp_target_id_audit_ref, require_target_session_id,
        validate_cdp_target_id,
    },
    tool, tool_router,
};
use crate::m1::{BrowserNetworkWaitEntry, mcp_error};
use rmcp::{RoleServer, schemars::JsonSchema, service::RequestContext};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use synapse_core::error_codes;

const REQUESTS_TOOL: &str = "browser_network_requests";
const REQUEST_TOOL: &str = "browser_network_request";
const DEFAULT_NETWORK_REQUEST_LIMIT: usize = 100;
const MAX_NETWORK_REQUEST_LIMIT: usize = 1000;
const MAX_NETWORK_FILTER_CHARS: usize = 8192;
const MAX_NETWORK_RESOURCE_TYPE_CHARS: usize = 128;
const MAX_NETWORK_REQUEST_ID_CHARS: usize = 2048;

/// Parameters for `browser_network_requests` (#1081): return captured Network
/// request records for the calling session's owned CDP target.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestsParams {
    /// CDP TargetID to read. Defaults to the active session CDP target. Must be
    /// owned by this session; the human foreground tab is never an implicit fallback.
    #[serde(default)]
    pub cdp_target_id: Option<String>,
    /// Browser HWND that owns the target. Required only with an explicit
    /// `cdp_target_id` and no active session target.
    #[serde(default)]
    pub window_hwnd: Option<i64>,
    /// Return only records whose latest update sequence is >= this cursor.
    #[serde(default)]
    pub since_seq: Option<u64>,
    /// Maximum records to return after filtering. Defaults to 100, max 1000.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Case-insensitive substring filter against request URL.
    #[serde(default)]
    pub url_contains: Option<String>,
    /// Regular expression filter against request URL.
    #[serde(default)]
    pub url_regex: Option<String>,
    /// Case-insensitive CDP Network resource type filter.
    #[serde(default)]
    pub resource_type: Option<String>,
    /// Minimum HTTP status, inclusive.
    #[serde(default)]
    pub status_min: Option<i64>,
    /// Maximum HTTP status, inclusive.
    #[serde(default)]
    pub status_max: Option<i64>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_seq: Option<u64>,
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_max: Option<i64>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestsResponse {
    pub session_id: String,
    pub window_hwnd: i64,
    pub transport: String,
    pub endpoint: String,
    pub cdp_target_id: String,
    pub capture_newly_armed: bool,
    pub next_cursor: u64,
    pub returned: usize,
    pub total_buffered: usize,
    pub dropped: u64,
    pub filters: BrowserNetworkRequestFilters,
    pub entries: Vec<BrowserNetworkWaitEntry>,
    pub readback_backend: String,
    pub backend_tier_used: String,
    pub required_foreground: bool,
}

/// Parameters for `browser_network_request` (#1082): inspect one captured
/// Network request by CDP request id, including response body by default.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestParams {
    /// CDP request id from `browser_network_requests`, `browser_wait_for_request`,
    /// or `browser_wait_for_response`.
    pub request_id: String,
    /// CDP TargetID to read. Defaults to the active session CDP target. Must be
    /// owned by this session; the human foreground tab is never an implicit fallback.
    #[serde(default)]
    pub cdp_target_id: Option<String>,
    /// Browser HWND that owns the target. Required only with an explicit
    /// `cdp_target_id` and no active session target.
    #[serde(default)]
    pub window_hwnd: Option<i64>,
    /// Include `Network.getResponseBody` readback. Defaults to true.
    #[serde(default = "default_true")]
    pub include_body: bool,
    /// Include `Network.getRequestPostData` when CDP reported post data.
    /// Defaults to true.
    #[serde(default = "default_true")]
    pub include_post_data: bool,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestResponse {
    pub session_id: String,
    pub window_hwnd: i64,
    pub transport: String,
    pub endpoint: String,
    pub cdp_target_id: String,
    pub capture_newly_armed: bool,
    pub request_id: String,
    pub include_body: bool,
    pub include_post_data: bool,
    pub entry: BrowserNetworkRequestDetail,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_post_data: Option<BrowserNetworkRequestPostData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<BrowserNetworkResponseBody>,
    pub readback_backend: String,
    pub backend_tier_used: String,
    pub required_foreground: bool,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestDetail {
    pub seq: u64,
    pub first_seq: u64,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_has_post_data: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_timestamp_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_wall_time_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiator: Option<Value>,
    pub redirects: Vec<BrowserNetworkResponseSnapshot>,
    pub response_received: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<BrowserNetworkResponseSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_timestamp_s: Option<f64>,
    pub loading_finished: bool,
    pub loading_failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_timestamp_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_timestamp_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoded_data_length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_error_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_canceled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_blocked_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_cors_error_status: Option<Value>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkResponseSnapshot {
    pub url: String,
    pub status: i64,
    pub status_text: String,
    pub headers: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<Value>,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_ip_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<i64>,
    pub encoded_data_length: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_disk_cache: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_service_worker: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_prefetch_cache: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_early_hints: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkRequestPostData {
    pub request_id: String,
    pub post_data: String,
    pub post_data_len_chars: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserNetworkResponseBody {
    pub request_id: String,
    pub body: String,
    pub base64_encoded: bool,
    pub body_len_chars: usize,
}

#[derive(Debug)]
struct NormalizedBrowserNetworkRequestsParams {
    since_seq: Option<u64>,
    limit: usize,
    url_contains: Option<String>,
    url_regex_pattern: Option<String>,
    url_regex: Option<regex::Regex>,
    resource_type: Option<String>,
    status_min: Option<i64>,
    status_max: Option<i64>,
}

#[derive(Debug)]
struct NormalizedBrowserNetworkRequestParams {
    request_id: String,
    include_body: bool,
    include_post_data: bool,
}

#[tool_router(router = browser_network_tool_router, vis = "pub(super)")]
impl SynapseService {
    #[tool(
        description = "List captured Network request records for the calling session's owned browser tab. Arms/reuses the target-scoped raw CDP Network buffer, returns cursor-delimited entries, and supports filters for url_contains, url_regex, resource_type, status_min/status_max, and since_seq. Target-scoped and background-safe: never activates the tab, never uses OS foreground input, and never falls back to the human foreground tab. Raw CDP only; the popup-safe normal Chrome extension bridge fails closed."
    )]
    pub async fn browser_network_requests(
        &self,
        params: Parameters<BrowserNetworkRequestsParams>,
        request_context: RequestContext<RoleServer>,
    ) -> Result<Json<BrowserNetworkRequestsResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = REQUESTS_TOOL,
            "tool.invocation kind=browser_network_requests"
        );
        let session_id = require_target_session_id(&request_context)?;
        let filters = validate_browser_network_requests_params(&params.0)?;
        let request_details = json!({
            "session_id": &session_id,
            "window_hwnd": params.0.window_hwnd,
            "requested_cdp_target": cdp_target_id_audit_ref(params.0.cdp_target_id.as_deref()),
            "since_seq": filters.since_seq,
            "limit": filters.limit,
            "url_contains_len": filters.url_contains.as_deref().map(str::len),
            "url_regex_len": filters.url_regex_pattern.as_deref().map(str::len),
            "resource_type": filters.resource_type.as_deref(),
            "status_min": filters.status_min,
            "status_max": filters.status_max,
            "required_foreground": false,
            "phase": "target_resolution",
        });
        let resolution = self.resolve_cdp_tab_mutation_target(
            REQUESTS_TOOL,
            &session_id,
            params.0.window_hwnd,
            params.0.cdp_target_id.as_deref(),
        );
        let (window_hwnd, cdp_target_id) = self.audit_cdp_target_resolution_result(
            REQUESTS_TOOL,
            &session_id,
            &request_details,
            resolution,
        )?;
        let request_details = json!({
            "session_id": &session_id,
            "window_hwnd": window_hwnd,
            "cdp_target_id": &cdp_target_id,
            "since_seq": filters.since_seq,
            "limit": filters.limit,
            "url_contains_len": filters.url_contains.as_deref().map(str::len),
            "url_regex_len": filters.url_regex_pattern.as_deref().map(str::len),
            "resource_type": filters.resource_type.as_deref(),
            "status_min": filters.status_min,
            "status_max": filters.status_max,
            "required_foreground": false,
        });
        self.audit_action_started_with_details_for_session(
            REQUESTS_TOOL,
            &request_details,
            &session_id,
        )?;
        let result = self
            .browser_network_requests_impl(&session_id, window_hwnd, &cdp_target_id, &filters)
            .await;
        self.audit_action_result_for_session(REQUESTS_TOOL, &result, &session_id)?;
        result.map(Json)
    }

    #[tool(
        description = "Inspect one captured Network request by CDP request_id for the calling session's owned browser tab. Reuses/arms the target-scoped raw CDP Network buffer, returns full request/response metadata, optional request post data, and a base64-aware Network.getResponseBody payload by default. Target-scoped and background-safe: never activates the tab, never uses OS foreground input, and never falls back to the human foreground tab. Raw CDP only; the popup-safe normal Chrome extension bridge fails closed."
    )]
    pub async fn browser_network_request(
        &self,
        params: Parameters<BrowserNetworkRequestParams>,
        request_context: RequestContext<RoleServer>,
    ) -> Result<Json<BrowserNetworkRequestResponse>, ErrorData> {
        tracing::info!(
            code = "MCP_TOOL_INVOCATION",
            kind = REQUEST_TOOL,
            "tool.invocation kind=browser_network_request"
        );
        let session_id = require_target_session_id(&request_context)?;
        let request = validate_browser_network_request_params(&params.0)?;
        let request_details = json!({
            "session_id": &session_id,
            "window_hwnd": params.0.window_hwnd,
            "requested_cdp_target": cdp_target_id_audit_ref(params.0.cdp_target_id.as_deref()),
            "request_id": &request.request_id,
            "include_body": request.include_body,
            "include_post_data": request.include_post_data,
            "required_foreground": false,
            "phase": "target_resolution",
        });
        let resolution = self.resolve_cdp_tab_mutation_target(
            REQUEST_TOOL,
            &session_id,
            params.0.window_hwnd,
            params.0.cdp_target_id.as_deref(),
        );
        let (window_hwnd, cdp_target_id) = self.audit_cdp_target_resolution_result(
            REQUEST_TOOL,
            &session_id,
            &request_details,
            resolution,
        )?;
        let request_details = json!({
            "session_id": &session_id,
            "window_hwnd": window_hwnd,
            "cdp_target_id": &cdp_target_id,
            "request_id": &request.request_id,
            "include_body": request.include_body,
            "include_post_data": request.include_post_data,
            "required_foreground": false,
        });
        self.audit_action_started_with_details_for_session(
            REQUEST_TOOL,
            &request_details,
            &session_id,
        )?;
        let result = self
            .browser_network_request_impl(&session_id, window_hwnd, &cdp_target_id, &request)
            .await;
        self.audit_action_result_for_session(REQUEST_TOOL, &result, &session_id)?;
        result.map(Json)
    }

    #[cfg(windows)]
    async fn browser_network_requests_impl(
        &self,
        session_id: &str,
        window_hwnd: i64,
        cdp_target_id: &str,
        filters: &NormalizedBrowserNetworkRequestsParams,
    ) -> Result<BrowserNetworkRequestsResponse, ErrorData> {
        let Some(endpoint) = synapse_a11y::endpoint_for_window(window_hwnd) else {
            return Err(browser_raw_cdp_required_error(REQUESTS_TOOL, window_hwnd));
        };
        let capture = synapse_a11y::network_capture_ensure(
            &endpoint,
            cdp_target_id,
            synapse_a11y::DEFAULT_NETWORK_BUFFER_CAPACITY,
        )
        .await
        .map_err(|error| {
            mcp_error(
                error.code(),
                format!("{REQUESTS_TOOL} raw CDP network capture failed: {error}"),
            )
        })?;
        let read = synapse_a11y::network_capture_read(
            cdp_target_id,
            &synapse_a11y::CdpNetworkReadFilter {
                since_seq: filters.since_seq,
                max: 0,
                ..Default::default()
            },
        )
        .ok_or_else(|| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("{REQUESTS_TOOL} network capture was not armed for target {cdp_target_id}"),
            )
        })?;
        let entries = filter_network_entries(read.entries.into_iter(), filters)
            .into_iter()
            .take(filters.limit)
            .map(|entry| browser_network_entry_to_wire(&entry))
            .collect::<Vec<_>>();
        tracing::info!(
            code = "CDP_BACKGROUND_NETWORK_REQUESTS",
            session_id = %session_id,
            hwnd = window_hwnd,
            endpoint = %endpoint,
            cdp_target_id,
            returned = entries.len(),
            total_buffered = read.total_buffered,
            next_cursor = read.next_cursor,
            "readback=Network.event_buffer(browser_network_requests) outcome=list_returned"
        );
        Ok(BrowserNetworkRequestsResponse {
            session_id: session_id.to_owned(),
            window_hwnd,
            transport: "raw_cdp".to_owned(),
            endpoint,
            cdp_target_id: cdp_target_id.to_owned(),
            capture_newly_armed: capture.newly_armed,
            next_cursor: read.next_cursor,
            returned: entries.len(),
            total_buffered: read.total_buffered,
            dropped: read.dropped,
            filters: filters.to_wire(),
            entries,
            readback_backend: "Network event buffer(browser_network_requests)".to_owned(),
            backend_tier_used: "cdp".to_owned(),
            required_foreground: false,
        })
    }

    #[cfg(windows)]
    async fn browser_network_request_impl(
        &self,
        session_id: &str,
        window_hwnd: i64,
        cdp_target_id: &str,
        request: &NormalizedBrowserNetworkRequestParams,
    ) -> Result<BrowserNetworkRequestResponse, ErrorData> {
        let Some(endpoint) = synapse_a11y::endpoint_for_window(window_hwnd) else {
            return Err(browser_raw_cdp_required_error(REQUEST_TOOL, window_hwnd));
        };
        let capture = synapse_a11y::network_capture_ensure(
            &endpoint,
            cdp_target_id,
            synapse_a11y::DEFAULT_NETWORK_BUFFER_CAPACITY,
        )
        .await
        .map_err(|error| {
            mcp_error(
                error.code(),
                format!("{REQUEST_TOOL} raw CDP network capture failed: {error}"),
            )
        })?;
        let read = synapse_a11y::network_capture_read(
            cdp_target_id,
            &synapse_a11y::CdpNetworkReadFilter {
                request_id: Some(request.request_id.as_str()),
                max: 1,
                ..Default::default()
            },
        )
        .ok_or_else(|| {
            mcp_error(
                error_codes::TOOL_INTERNAL_ERROR,
                format!("{REQUEST_TOOL} network capture was not armed for target {cdp_target_id}"),
            )
        })?;
        let Some(entry) = read.entries.into_iter().next() else {
            return Err(mcp_error(
                error_codes::TOOL_PARAMS_INVALID,
                format!(
                    "{REQUEST_TOOL} request_id {:?} is not present in the target network buffer",
                    request.request_id
                ),
            ));
        };
        let request_post_data = if request.include_post_data
            && entry.request_has_post_data.unwrap_or(false)
        {
            Some(
                synapse_a11y::network_request_post_data(cdp_target_id, &entry.request_id)
                    .await
                    .map_err(|error| {
                        mcp_error(
                            error.code(),
                            format!(
                                "{REQUEST_TOOL} raw CDP request post data failed request_id={}: {error}",
                                entry.request_id
                            ),
                        )
                    })
                    .map(browser_network_post_data_to_wire)?,
            )
        } else {
            None
        };
        let response_body = if request.include_body {
            require_response_body_available(&entry)?;
            Some(
                synapse_a11y::network_response_body(cdp_target_id, &entry.request_id)
                    .await
                    .map_err(|error| {
                        mcp_error(
                            error.code(),
                            format!(
                                "{REQUEST_TOOL} raw CDP response body failed request_id={}: {error}",
                                entry.request_id
                            ),
                        )
                    })
                    .map(browser_network_response_body_to_wire)?,
            )
        } else {
            None
        };
        tracing::info!(
            code = "CDP_BACKGROUND_NETWORK_REQUEST",
            session_id = %session_id,
            hwnd = window_hwnd,
            endpoint = %endpoint,
            cdp_target_id,
            request_id = %entry.request_id,
            include_body = request.include_body,
            response_body_returned = response_body.is_some(),
            request_post_data_returned = request_post_data.is_some(),
            "readback=Network.getResponseBody(browser_network_request) outcome=request_returned"
        );
        Ok(BrowserNetworkRequestResponse {
            session_id: session_id.to_owned(),
            window_hwnd,
            transport: "raw_cdp".to_owned(),
            endpoint,
            cdp_target_id: cdp_target_id.to_owned(),
            capture_newly_armed: capture.newly_armed,
            request_id: entry.request_id.clone(),
            include_body: request.include_body,
            include_post_data: request.include_post_data,
            entry: browser_network_request_detail_to_wire(&entry),
            request_post_data,
            response_body,
            readback_backend:
                "Network event buffer + Network.getResponseBody(browser_network_request)".to_owned(),
            backend_tier_used: "cdp".to_owned(),
            required_foreground: false,
        })
    }

    #[cfg(not(windows))]
    async fn browser_network_request_impl(
        &self,
        _session_id: &str,
        _window_hwnd: i64,
        _cdp_target_id: &str,
        _request: &NormalizedBrowserNetworkRequestParams,
    ) -> Result<BrowserNetworkRequestResponse, ErrorData> {
        Err(mcp_error(
            error_codes::A11Y_NOT_AVAILABLE,
            "browser_network_request is only available on Windows in this build",
        ))
    }

    #[cfg(not(windows))]
    async fn browser_network_requests_impl(
        &self,
        _session_id: &str,
        _window_hwnd: i64,
        _cdp_target_id: &str,
        _filters: &NormalizedBrowserNetworkRequestsParams,
    ) -> Result<BrowserNetworkRequestsResponse, ErrorData> {
        Err(mcp_error(
            error_codes::A11Y_NOT_AVAILABLE,
            "browser_network_requests is only available on Windows in this build",
        ))
    }
}

impl NormalizedBrowserNetworkRequestsParams {
    fn to_wire(&self) -> BrowserNetworkRequestFilters {
        BrowserNetworkRequestFilters {
            since_seq: self.since_seq,
            limit: self.limit,
            url_contains: self.url_contains.clone(),
            url_regex: self.url_regex_pattern.clone(),
            resource_type: self.resource_type.clone(),
            status_min: self.status_min,
            status_max: self.status_max,
        }
    }
}

fn default_true() -> bool {
    true
}

fn validate_browser_network_requests_params(
    params: &BrowserNetworkRequestsParams,
) -> Result<NormalizedBrowserNetworkRequestsParams, ErrorData> {
    if let Some(target_id) = params.cdp_target_id.as_deref() {
        validate_cdp_target_id(target_id)?;
    }
    let limit = params.limit.unwrap_or(DEFAULT_NETWORK_REQUEST_LIMIT);
    if !(1..=MAX_NETWORK_REQUEST_LIMIT).contains(&limit) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} limit must be 1..={MAX_NETWORK_REQUEST_LIMIT}"),
        ));
    }
    if params.url_contains.is_some() && params.url_regex.is_some() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} accepts url_contains or url_regex, not both"),
        ));
    }
    let url_contains = validate_text_filter("url_contains", params.url_contains.as_deref())?;
    let url_regex_pattern = validate_text_filter("url_regex", params.url_regex.as_deref())?;
    let url_regex = url_regex_pattern
        .as_deref()
        .map(|pattern| {
            regex::Regex::new(pattern).map_err(|error| {
                mcp_error(
                    error_codes::TOOL_PARAMS_INVALID,
                    format!("{REQUESTS_TOOL} url_regex is invalid: {error}"),
                )
            })
        })
        .transpose()?;
    let resource_type = validate_resource_type(params.resource_type.as_deref())?;
    validate_status_bound("status_min", params.status_min)?;
    validate_status_bound("status_max", params.status_max)?;
    if let (Some(min), Some(max)) = (params.status_min, params.status_max)
        && min > max
    {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} status_min must be <= status_max"),
        ));
    }
    Ok(NormalizedBrowserNetworkRequestsParams {
        since_seq: params.since_seq,
        limit,
        url_contains,
        url_regex_pattern,
        url_regex,
        resource_type,
        status_min: params.status_min,
        status_max: params.status_max,
    })
}

fn validate_browser_network_request_params(
    params: &BrowserNetworkRequestParams,
) -> Result<NormalizedBrowserNetworkRequestParams, ErrorData> {
    if let Some(target_id) = params.cdp_target_id.as_deref() {
        validate_cdp_target_id(target_id)?;
    }
    let request_id = validate_request_id(&params.request_id)?;
    Ok(NormalizedBrowserNetworkRequestParams {
        request_id,
        include_body: params.include_body,
        include_post_data: params.include_post_data,
    })
}

fn validate_request_id(request_id: &str) -> Result<String, ErrorData> {
    if request_id.is_empty() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUEST_TOOL} request_id must not be empty"),
        ));
    }
    if request_id.trim() != request_id {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUEST_TOOL} request_id must not contain leading or trailing whitespace"),
        ));
    }
    if request_id.contains('\0') || request_id.chars().any(char::is_control) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUEST_TOOL} request_id must not contain control characters"),
        ));
    }
    if request_id.chars().count() > MAX_NETWORK_REQUEST_ID_CHARS {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "{REQUEST_TOOL} request_id must be at most {MAX_NETWORK_REQUEST_ID_CHARS} Unicode scalar values"
            ),
        ));
    }
    Ok(request_id.to_owned())
}

fn validate_text_filter(field: &str, value: Option<&str>) -> Result<Option<String>, ErrorData> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} {field} must not be empty"),
        ));
    }
    if value.contains('\0') {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} {field} must not contain NUL"),
        ));
    }
    if value.chars().count() > MAX_NETWORK_FILTER_CHARS {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "{REQUESTS_TOOL} {field} must be at most {MAX_NETWORK_FILTER_CHARS} Unicode scalar values"
            ),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn validate_resource_type(value: Option<&str>) -> Result<Option<String>, ErrorData> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} resource_type must not be empty"),
        ));
    }
    if value.trim() != value {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "{REQUESTS_TOOL} resource_type must not contain leading or trailing whitespace"
            ),
        ));
    }
    if value.contains('\0') || value.chars().any(char::is_control) {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} resource_type must not contain control characters"),
        ));
    }
    if value.chars().count() > MAX_NETWORK_RESOURCE_TYPE_CHARS {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!(
                "{REQUESTS_TOOL} resource_type must be at most {MAX_NETWORK_RESOURCE_TYPE_CHARS} Unicode scalar values"
            ),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn validate_status_bound(field: &str, value: Option<i64>) -> Result<(), ErrorData> {
    if let Some(value) = value
        && !(0..=999).contains(&value)
    {
        return Err(mcp_error(
            error_codes::TOOL_PARAMS_INVALID,
            format!("{REQUESTS_TOOL} {field} must be 0..=999"),
        ));
    }
    Ok(())
}

fn filter_network_entries(
    entries: impl Iterator<Item = synapse_a11y::CdpNetworkEntry>,
    filters: &NormalizedBrowserNetworkRequestsParams,
) -> Vec<synapse_a11y::CdpNetworkEntry> {
    entries
        .filter(|entry| network_entry_matches(entry, filters))
        .collect()
}

fn network_entry_matches(
    entry: &synapse_a11y::CdpNetworkEntry,
    filters: &NormalizedBrowserNetworkRequestsParams,
) -> bool {
    if let Some(resource_type) = filters.resource_type.as_deref()
        && !entry
            .resource_type
            .as_deref()
            .is_some_and(|entry_type| entry_type.eq_ignore_ascii_case(resource_type))
    {
        return false;
    }
    let status = entry.response.as_ref().map(|response| response.status);
    if let Some(min) = filters.status_min
        && !status.is_some_and(|status| status >= min)
    {
        return false;
    }
    if let Some(max) = filters.status_max
        && !status.is_some_and(|status| status <= max)
    {
        return false;
    }
    if let Some(needle) = filters.url_contains.as_deref()
        && !entry
            .url
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&needle.to_lowercase())
    {
        return false;
    }
    if let Some(regex) = filters.url_regex.as_ref()
        && !entry.url.as_deref().is_some_and(|url| regex.is_match(url))
    {
        return false;
    }
    true
}

fn browser_network_entry_to_wire(entry: &synapse_a11y::CdpNetworkEntry) -> BrowserNetworkWaitEntry {
    let response = entry.response.as_ref();
    BrowserNetworkWaitEntry {
        seq: entry.seq,
        request_id: entry.request_id.clone(),
        url: entry.url.clone(),
        method: entry.method.clone(),
        resource_type: entry.resource_type.clone(),
        request_headers: entry.request_headers.clone(),
        response_received: entry.response_received,
        response_url: response.map(|response| response.url.clone()),
        status: response.map(|response| response.status),
        status_text: response.map(|response| response.status_text.clone()),
        response_headers: response.map(|response| response.headers.clone()),
        response_timing: response.and_then(|response| response.timing.clone()),
        protocol: response.and_then(|response| response.protocol.clone()),
        remote_ip_address: response.and_then(|response| response.remote_ip_address.clone()),
        remote_port: response.and_then(|response| response.remote_port),
        encoded_data_length: entry
            .encoded_data_length
            .or_else(|| response.map(|response| response.encoded_data_length)),
        loading_finished: entry.loading_finished,
        loading_failed: entry.loading_failed,
        failure_error_text: entry.failure_error_text.clone(),
    }
}

fn browser_network_request_detail_to_wire(
    entry: &synapse_a11y::CdpNetworkEntry,
) -> BrowserNetworkRequestDetail {
    BrowserNetworkRequestDetail {
        seq: entry.seq,
        first_seq: entry.first_seq,
        request_id: entry.request_id.clone(),
        loader_id: entry.loader_id.clone(),
        frame_id: entry.frame_id.clone(),
        document_url: entry.document_url.clone(),
        url: entry.url.clone(),
        method: entry.method.clone(),
        resource_type: entry.resource_type.clone(),
        request_headers: entry.request_headers.clone(),
        request_has_post_data: entry.request_has_post_data,
        request_timestamp_s: entry.request_timestamp_s,
        request_wall_time_ms: entry.request_wall_time_ms,
        initiator: entry.initiator.clone(),
        redirects: entry
            .redirects
            .iter()
            .map(browser_network_response_snapshot_to_wire)
            .collect(),
        response_received: entry.response_received,
        response: entry
            .response
            .as_ref()
            .map(browser_network_response_snapshot_to_wire),
        response_timestamp_s: entry.response_timestamp_s,
        loading_finished: entry.loading_finished,
        loading_failed: entry.loading_failed,
        finished_timestamp_s: entry.finished_timestamp_s,
        failed_timestamp_s: entry.failed_timestamp_s,
        encoded_data_length: entry.encoded_data_length,
        failure_error_text: entry.failure_error_text.clone(),
        failure_canceled: entry.failure_canceled,
        failure_blocked_reason: entry.failure_blocked_reason.clone(),
        failure_cors_error_status: entry.failure_cors_error_status.clone(),
    }
}

fn browser_network_response_snapshot_to_wire(
    response: &synapse_a11y::CdpNetworkResponseSnapshot,
) -> BrowserNetworkResponseSnapshot {
    BrowserNetworkResponseSnapshot {
        url: response.url.clone(),
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response.headers.clone(),
        request_headers: response.request_headers.clone(),
        mime_type: response.mime_type.clone(),
        protocol: response.protocol.clone(),
        remote_ip_address: response.remote_ip_address.clone(),
        remote_port: response.remote_port,
        encoded_data_length: response.encoded_data_length,
        timing: response.timing.clone(),
        response_time_ms: response.response_time_ms,
        from_disk_cache: response.from_disk_cache,
        from_service_worker: response.from_service_worker,
        from_prefetch_cache: response.from_prefetch_cache,
        from_early_hints: response.from_early_hints,
        timestamp_s: response.timestamp_s,
        resource_type: response.resource_type.clone(),
    }
}

fn browser_network_response_body_to_wire(
    body: synapse_a11y::CdpNetworkResponseBody,
) -> BrowserNetworkResponseBody {
    let body_len_chars = body.body.chars().count();
    BrowserNetworkResponseBody {
        request_id: body.request_id,
        body: body.body,
        base64_encoded: body.base64_encoded,
        body_len_chars,
    }
}

fn browser_network_post_data_to_wire(
    post_data: synapse_a11y::CdpNetworkRequestPostData,
) -> BrowserNetworkRequestPostData {
    let post_data_len_chars = post_data.post_data.chars().count();
    BrowserNetworkRequestPostData {
        request_id: post_data.request_id,
        post_data: post_data.post_data,
        post_data_len_chars,
    }
}

fn require_response_body_available(entry: &synapse_a11y::CdpNetworkEntry) -> Result<(), ErrorData> {
    if !entry.response_received {
        return Err(mcp_error(
            error_codes::A11Y_CDP_AXTREE_FAILED,
            format!(
                "{REQUEST_TOOL} response body is unavailable for request_id={}: no responseReceived event captured",
                entry.request_id
            ),
        ));
    }
    if entry.loading_failed {
        return Err(mcp_error(
            error_codes::A11Y_CDP_AXTREE_FAILED,
            format!(
                "{REQUEST_TOOL} response body is unavailable for request_id={}: loadingFailed {:?}",
                entry.request_id, entry.failure_error_text
            ),
        ));
    }
    if !entry.loading_finished {
        return Err(mcp_error(
            error_codes::A11Y_CDP_AXTREE_FAILED,
            format!(
                "{REQUEST_TOOL} response body is unavailable for request_id={}: loadingFinished has not been captured yet",
                entry.request_id
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        seq: u64,
        request_id: &str,
        url: &str,
        resource_type: &str,
        status: Option<i64>,
    ) -> synapse_a11y::CdpNetworkEntry {
        let response = status.map(|status| synapse_a11y::CdpNetworkResponseSnapshot {
            url: url.to_owned(),
            status,
            status_text: "OK".to_owned(),
            headers: json!({"content-type": "application/json"}),
            request_headers: None,
            mime_type: "application/json".to_owned(),
            protocol: Some("h2".to_owned()),
            remote_ip_address: Some("127.0.0.1".to_owned()),
            remote_port: Some(443),
            encoded_data_length: 42.0,
            timing: Some(json!({"requestTime": 1.0})),
            response_time_ms: None,
            from_disk_cache: None,
            from_service_worker: None,
            from_prefetch_cache: None,
            from_early_hints: None,
            timestamp_s: Some(2.0),
            resource_type: Some(resource_type.to_owned()),
        });
        synapse_a11y::CdpNetworkEntry {
            seq,
            first_seq: seq,
            request_id: request_id.to_owned(),
            loader_id: Some("loader".to_owned()),
            frame_id: Some("frame".to_owned()),
            document_url: Some("https://example.test/".to_owned()),
            url: Some(url.to_owned()),
            method: Some("GET".to_owned()),
            resource_type: Some(resource_type.to_owned()),
            request_headers: Some(json!({"accept": "*/*"})),
            request_has_post_data: None,
            request_timestamp_s: Some(1.0),
            request_wall_time_ms: Some(1_710_000_000_000.0),
            initiator: None,
            redirects: Vec::new(),
            response_timestamp_s: response.as_ref().and_then(|r| r.timestamp_s),
            response_received: response.is_some(),
            response,
            loading_finished: true,
            loading_failed: false,
            finished_timestamp_s: Some(3.0),
            failed_timestamp_s: None,
            encoded_data_length: Some(84.0),
            failure_error_text: None,
            failure_canceled: None,
            failure_blocked_reason: None,
            failure_cors_error_status: None,
        }
    }

    #[test]
    fn browser_network_requests_validation_edges() {
        let ok = validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
            cdp_target_id: Some("target-123".to_owned()),
            since_seq: Some(7),
            limit: Some(MAX_NETWORK_REQUEST_LIMIT),
            url_regex: Some(r"^https://example\.test/api".to_owned()),
            resource_type: Some("XHR".to_owned()),
            status_min: Some(200),
            status_max: Some(299),
            ..Default::default()
        })
        .expect("valid params pass");
        assert_eq!(ok.since_seq, Some(7));
        assert_eq!(ok.limit, MAX_NETWORK_REQUEST_LIMIT);
        assert!(
            ok.url_regex
                .as_ref()
                .unwrap()
                .is_match("https://example.test/api")
        );
        assert_eq!(ok.resource_type.as_deref(), Some("XHR"));

        for error in [
            validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
                limit: Some(0),
                ..Default::default()
            })
            .expect_err("zero limit must be rejected"),
            validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
                url_contains: Some("api".to_owned()),
                url_regex: Some("api".to_owned()),
                ..Default::default()
            })
            .expect_err("ambiguous URL filters must be rejected"),
            validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
                url_regex: Some("(".to_owned()),
                ..Default::default()
            })
            .expect_err("invalid URL regex must be rejected"),
            validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
                resource_type: Some(" XHR".to_owned()),
                ..Default::default()
            })
            .expect_err("resource type whitespace must be rejected"),
            validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
                status_min: Some(500),
                status_max: Some(200),
                ..Default::default()
            })
            .expect_err("inverted status range must be rejected"),
        ] {
            let code = error
                .data
                .as_ref()
                .and_then(|data| data.get("code"))
                .and_then(serde_json::Value::as_str);
            assert_eq!(code, Some(error_codes::TOOL_PARAMS_INVALID));
        }
    }

    #[test]
    fn browser_network_requests_filters_entries_after_cursor_read() {
        let filters = validate_browser_network_requests_params(&BrowserNetworkRequestsParams {
            url_contains: Some("/api/".to_owned()),
            resource_type: Some("XHR".to_owned()),
            status_min: Some(200),
            status_max: Some(299),
            ..Default::default()
        })
        .expect("filters validate");
        let filtered = filter_network_entries(
            vec![
                entry(1, "doc", "https://example.test/", "Document", Some(200)),
                entry(
                    2,
                    "api-ok",
                    "https://example.test/api/users",
                    "XHR",
                    Some(204),
                ),
                entry(
                    3,
                    "api-err",
                    "https://example.test/api/fail",
                    "XHR",
                    Some(500),
                ),
            ]
            .into_iter(),
            &filters,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].request_id, "api-ok");
        let wire = browser_network_entry_to_wire(&filtered[0]);
        assert_eq!(wire.status, Some(204));
        assert_eq!(
            wire.response_headers,
            Some(json!({"content-type": "application/json"}))
        );
        assert_eq!(wire.encoded_data_length, Some(84.0));
    }

    #[test]
    fn browser_network_request_validation_edges() {
        let ok = validate_browser_network_request_params(&BrowserNetworkRequestParams {
            request_id: "1234.56".to_owned(),
            cdp_target_id: Some("target-123".to_owned()),
            window_hwnd: Some(100),
            include_body: true,
            include_post_data: true,
        })
        .expect("valid request params pass");
        assert_eq!(ok.request_id, "1234.56");
        assert!(ok.include_body);
        assert!(ok.include_post_data);

        for error in [
            validate_browser_network_request_params(&BrowserNetworkRequestParams {
                request_id: String::new(),
                cdp_target_id: None,
                window_hwnd: None,
                include_body: true,
                include_post_data: true,
            })
            .expect_err("empty request id must be rejected"),
            validate_browser_network_request_params(&BrowserNetworkRequestParams {
                request_id: " request ".to_owned(),
                cdp_target_id: None,
                window_hwnd: None,
                include_body: true,
                include_post_data: true,
            })
            .expect_err("request id whitespace must be rejected"),
            validate_browser_network_request_params(&BrowserNetworkRequestParams {
                request_id: "bad\nid".to_owned(),
                cdp_target_id: None,
                window_hwnd: None,
                include_body: true,
                include_post_data: true,
            })
            .expect_err("request id control chars must be rejected"),
        ] {
            let code = error
                .data
                .as_ref()
                .and_then(|data| data.get("code"))
                .and_then(serde_json::Value::as_str);
            assert_eq!(code, Some(error_codes::TOOL_PARAMS_INVALID));
        }
    }

    #[test]
    fn browser_network_request_detail_maps_full_entry_and_body_metadata() {
        let mut captured = entry(
            9,
            "api-ok",
            "https://example.test/api/users",
            "XHR",
            Some(200),
        );
        captured.first_seq = 7;
        captured.request_has_post_data = Some(true);
        captured.initiator = Some(json!({"type": "script"}));
        captured
            .redirects
            .push(synapse_a11y::CdpNetworkResponseSnapshot {
                url: "https://example.test/old".to_owned(),
                status: 302,
                status_text: "Found".to_owned(),
                headers: json!({"location": "/api/users"}),
                request_headers: None,
                mime_type: "text/html".to_owned(),
                protocol: Some("h2".to_owned()),
                remote_ip_address: Some("127.0.0.1".to_owned()),
                remote_port: Some(443),
                encoded_data_length: 10.0,
                timing: None,
                response_time_ms: Some(3.0),
                from_disk_cache: Some(false),
                from_service_worker: Some(false),
                from_prefetch_cache: Some(false),
                from_early_hints: Some(false),
                timestamp_s: Some(1.5),
                resource_type: Some("XHR".to_owned()),
            });

        let detail = browser_network_request_detail_to_wire(&captured);
        assert_eq!(detail.seq, 9);
        assert_eq!(detail.first_seq, 7);
        assert_eq!(detail.request_id, "api-ok");
        assert_eq!(detail.request_has_post_data, Some(true));
        assert_eq!(detail.initiator, Some(json!({"type": "script"})));
        assert_eq!(detail.redirects.len(), 1);
        assert_eq!(detail.response.as_ref().map(|r| r.status), Some(200));

        let body = browser_network_response_body_to_wire(synapse_a11y::CdpNetworkResponseBody {
            request_id: "api-ok".to_owned(),
            body: "{\"ok\":true}".to_owned(),
            base64_encoded: false,
        });
        assert_eq!(body.body_len_chars, 11);
        assert!(!body.base64_encoded);

        let post_data =
            browser_network_post_data_to_wire(synapse_a11y::CdpNetworkRequestPostData {
                request_id: "api-ok".to_owned(),
                post_data: "{\"name\":\"Ada\"}".to_owned(),
            });
        assert_eq!(post_data.post_data_len_chars, 14);
    }

    #[test]
    fn browser_network_request_body_requires_completed_response() {
        let mut pending = entry(1, "pending", "https://example.test/api", "XHR", Some(200));
        pending.loading_finished = false;
        let error = require_response_body_available(&pending)
            .expect_err("pending response body must be rejected");
        let code = error
            .data
            .as_ref()
            .and_then(|data| data.get("code"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(code, Some(error_codes::A11Y_CDP_AXTREE_FAILED));
    }
}
