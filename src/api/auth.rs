use crate::app::state::AppState;
use crate::config::schema::ExternalIdentityConfig;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdminRole {
    ReadOnly,
    Operator,
    ConfigEditor,
    CredentialManager,
    Admin,
}

impl AdminRole {
    pub fn as_str(self) -> &'static str {
        match self {
            AdminRole::ReadOnly => "read_only",
            AdminRole::Operator => "operator",
            AdminRole::ConfigEditor => "config_editor",
            AdminRole::CredentialManager => "credential_manager",
            AdminRole::Admin => "admin",
        }
    }

    pub fn can_manage_credentials(self) -> bool {
        matches!(self, AdminRole::CredentialManager | AdminRole::Admin)
    }
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub role: AdminRole,
    pub source: AuthSource,
}

impl AuthContext {
    pub fn audit_actor(&self) -> String {
        match &self.email {
            Some(email) if !email.is_empty() => format!("{}:{}", self.source.as_str(), email),
            _ => format!("{}:{}", self.source.as_str(), self.subject),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AuthSource {
    MasterKey,
    UiSession,
    ExternalIdentity,
}

impl AuthSource {
    fn as_str(&self) -> &'static str {
        match self {
            AuthSource::MasterKey => "master_key",
            AuthSource::UiSession => "ui_session",
            AuthSource::ExternalIdentity => "external_identity",
        }
    }
}

/// Optional API key authentication middleware.
/// If `server.master_api_key` is set, all requests must include
/// `Authorization: Bearer <key>`.
/// Query-string API keys are accepted only when explicitly enabled.
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let config = state.config();
    let master_key = &config.server.master_api_key;
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    if master_key.is_empty() && !config.server.external_identity.enabled {
        // Auth disabled.
        return Ok(next.run(req).await);
    }

    let required_role = required_admin_role(&method, &path);

    if master_key.is_empty() && config.server.external_identity.enabled && required_role.is_none() {
        return Ok(next.run(req).await);
    }

    // Check Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let has_authorization_header = !auth_header.is_empty();

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        if token == master_key {
            return run_authorized(
                req,
                next,
                AuthContext {
                    subject: "master".into(),
                    email: None,
                    display_name: Some("Master key".into()),
                    role: AdminRole::Admin,
                    source: AuthSource::MasterKey,
                },
                required_role,
            )
            .await;
        }
    }

    if config.server.ui_session_auth {
        if let Some(cookie) = req.headers().get("Cookie").and_then(|v| v.to_str().ok()) {
            let session = cookie.split(';').find_map(|part| {
                let mut pieces = part.trim().splitn(2, '=');
                (pieces.next() == Some("tokenscavenger_session"))
                    .then(|| pieces.next())
                    .flatten()
            });
            if let Some(token) = session {
                if state.ui_sessions.contains_key(token) {
                    return run_authorized(
                        req,
                        next,
                        AuthContext {
                            subject: "browser-session".into(),
                            email: None,
                            display_name: Some("Browser session".into()),
                            role: AdminRole::Admin,
                            source: AuthSource::UiSession,
                        },
                        required_role,
                    )
                    .await;
                }
            }
        }
    }

    if config.server.external_identity.enabled && is_admin_or_ui_request(&path) {
        if let Some(context) =
            external_identity_context(req.headers(), &config.server.external_identity)
        {
            return run_authorized(req, next, context, required_role).await;
        }
    }

    if config.server.allow_query_api_keys {
        if let Some(query_key) = req.uri().query().and_then(|q| {
            q.split('&').find_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                if parts.next() == Some("api_key") {
                    parts.next()
                } else {
                    None
                }
            })
        }) {
            if query_key == master_key {
                return run_authorized(
                    req,
                    next,
                    AuthContext {
                        subject: "master".into(),
                        email: None,
                        display_name: Some("Master key".into()),
                        role: AdminRole::Admin,
                        source: AuthSource::MasterKey,
                    },
                    required_role,
                )
                .await;
            }
        }
    }

    if config.server.ui_session_auth && is_ui_request(&path) {
        return Ok((StatusCode::SEE_OTHER, [(header::LOCATION, "/ui/login")]).into_response());
    }

    warn!(
        method = %method,
        path = %path,
        has_authorization_header,
        "Authentication failed: invalid or missing API key"
    );
    Err(StatusCode::UNAUTHORIZED)
}

async fn run_authorized(
    mut req: Request<Body>,
    next: Next,
    context: AuthContext,
    required_role: Option<AdminRole>,
) -> Result<Response, StatusCode> {
    if let Some(required) = required_role {
        if context.role < required {
            warn!(
                subject = %context.subject,
                role = %context.role.as_str(),
                required_role = %required.as_str(),
                "Authorization failed: insufficient admin role"
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }
    req.extensions_mut().insert(context);
    Ok(next.run(req).await)
}

fn is_ui_request(path: &str) -> bool {
    path == "/ui" || path.starts_with("/ui/")
}

fn is_admin_or_ui_request(path: &str) -> bool {
    is_ui_request(path) || path.starts_with("/admin/")
}

fn required_admin_role(method: &Method, path: &str) -> Option<AdminRole> {
    if is_ui_request(path) {
        return Some(AdminRole::ReadOnly);
    }
    if !path.starts_with("/admin/") {
        return None;
    }
    if method == Method::GET {
        return Some(AdminRole::ReadOnly);
    }
    if method == Method::POST && path.ends_with("/test") {
        return Some(AdminRole::Operator);
    }
    if method == Method::POST && path == "/admin/providers/discovery/refresh" {
        return Some(AdminRole::Operator);
    }
    Some(AdminRole::ConfigEditor)
}

fn external_identity_context(
    headers: &HeaderMap,
    config: &ExternalIdentityConfig,
) -> Option<AuthContext> {
    let subject = header_value(headers, &config.user_header)
        .or_else(|| header_value(headers, &config.email_header))?;
    let email = header_value(headers, &config.email_header);
    let display_name = header_value(headers, &config.name_header);
    let groups = header_value(headers, &config.groups_header)
        .map(|raw| split_groups(&raw, &config.group_delimiter))
        .unwrap_or_default();
    let role = role_from_groups(&groups, config)?;

    Some(AuthContext {
        subject,
        email,
        display_name,
        role,
        source: AuthSource::ExternalIdentity,
    })
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn split_groups(raw: &str, delimiter: &str) -> Vec<String> {
    let delimiter = if delimiter.is_empty() { "," } else { delimiter };
    raw.split(delimiter)
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn role_from_groups(groups: &[String], config: &ExternalIdentityConfig) -> Option<AdminRole> {
    if matches_any(groups, &config.admin_groups) {
        Some(AdminRole::Admin)
    } else if matches_any(groups, &config.credential_manager_groups) {
        Some(AdminRole::CredentialManager)
    } else if matches_any(groups, &config.config_editor_groups) {
        Some(AdminRole::ConfigEditor)
    } else if matches_any(groups, &config.operator_groups) {
        Some(AdminRole::Operator)
    } else if matches_any(groups, &config.read_only_groups) {
        Some(AdminRole::ReadOnly)
    } else {
        None
    }
}

fn matches_any(groups: &[String], allowed: &[String]) -> bool {
    groups
        .iter()
        .any(|group| allowed.iter().any(|allowed| group == allowed))
}
