use crate::AppState;
use async_trait::async_trait;
use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
    response::Redirect,
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── JWT Claims ────────────────────────────────────────────────────────────

/// JWT claims embedded in the auth token.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64,     // user ID
    pub email: String,
    pub role: String, // "admin" or "user"
    pub exp: usize,   // expiry (Unix timestamp)
    pub iat: usize,   // issued at
    #[serde(default)] // backward compat with tokens issued before this field existed
    pub fpc: bool,    // force password change
}

/// Create a signed JWT for the given user.
pub fn create_jwt(
    user_id: i64,
    email: &str,
    role: &str,
    secret: &str,
    duration_hours: u64,
    force_password_change: bool,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let exp = (now + chrono::Duration::hours(duration_hours as i64)).timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        email: email.to_string(),
        role: role.to_string(),
        exp,
        iat: now.timestamp() as usize,
        fpc: force_password_change,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Decode and validate a JWT. Returns claims if valid.
pub fn verify_jwt(token: &str, secret: &str) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims)
}

// ── AuthUser extractor ───────────────────────────────────────────────────

/// Extractor that enforces authentication. Carries user identity from the JWT.
#[allow(dead_code)]
pub struct AuthUser {
    pub user_id: i64,
    pub email: String,
    pub role: String,
    pub force_password_change: bool,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

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

        let claims = jar
            .get("auth_token")
            .and_then(|cookie| verify_jwt(cookie.value(), &state.config.jwt_secret));

        match claims {
            Some(c) => {
                // If forced to change password, only allow change-password and logout routes
                if c.fpc {
                    let path = parts.uri.path();
                    if path != "/admin/change-password" && path != "/admin/logout" {
                        return Err(Redirect::to("/admin/change-password"));
                    }
                }
                Ok(AuthUser {
                    user_id: c.sub,
                    email: c.email,
                    role: c.role,
                    force_password_change: c.fpc,
                })
            }
            None => Err(Redirect::to("/admin/login")),
        }
    }
}

// ── AdminUser extractor ──────────────────────────────────────────────────

/// Extractor that requires admin role. Redirects non-admins to dashboard.
#[allow(dead_code)]
pub struct AdminUser {
    pub user_id: i64,
    pub email: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        if auth.is_admin() {
            Ok(AdminUser {
                user_id: auth.user_id,
                email: auth.email,
            })
        } else {
            Err(Redirect::to("/admin/dashboard"))
        }
    }
}
