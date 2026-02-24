use crate::AppState;
use async_trait::async_trait;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    response::Redirect,
};
use axum_extra::extract::CookieJar;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Session Store ──────────────────────────────────────────────────────────

/// In-memory session store. Each entry maps a session token (UUID) to the
/// instant it was created. Tokens expire after `session_duration`.
pub struct SessionStore {
    sessions: RwLock<HashMap<String, Instant>>,
    pub session_duration: Duration,
}

impl SessionStore {
    pub fn new(session_duration_hours: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            session_duration: Duration::from_secs(session_duration_hours * 3600),
        }
    }

    /// Create a new session and return its token.
    pub async fn create(&self) -> String {
        let token = Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().await;
        // Opportunistically prune expired sessions on every login
        sessions.retain(|_, created_at| created_at.elapsed() < self.session_duration);
        sessions.insert(token.clone(), Instant::now());
        token
    }

    /// Return `true` if the token exists and has not expired.
    pub async fn is_valid(&self, token: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions
            .get(token)
            .map(|created_at| created_at.elapsed() < self.session_duration)
            .unwrap_or(false)
    }

    /// Invalidate a specific session (logout).
    pub async fn remove(&self, token: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(token);
    }
}

// ── AuthUser extractor ─────────────────────────────────────────────────────

/// Extractor that enforces authentication on any handler that includes it as
/// a parameter. If the request carries a valid `session_id` cookie the
/// extractor succeeds; otherwise it short-circuits with a redirect to the
/// login page so the handler never runs.
pub struct AuthUser;

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = Arc::<AppState>::from_ref(state);
        let jar = CookieJar::from_headers(&parts.headers);

        let valid = if let Some(cookie) = jar.get("session_id") {
            state.sessions.is_valid(cookie.value()).await
        } else {
            false
        };

        if valid {
            Ok(AuthUser)
        } else {
            Err(Redirect::to("/admin/login"))
        }
    }
}
