use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use axum::async_trait;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::state::AppState;

pub const SESSION_COOKIE: &str = "trader_session";
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30; // 30 days

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn create_session(pool: &SqlitePool, user_id: &str) -> sqlx::Result<String> {
    let token = new_token();
    let now = Utc::now().timestamp();
    sqlx::query("INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(&token)
        .bind(user_id)
        .bind(now)
        .bind(now + SESSION_TTL_SECS)
        .execute(pool)
        .await?;
    Ok(token)
}

pub async fn delete_session(pool: &SqlitePool, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?").bind(token).execute(pool).await?;
    Ok(())
}

async fn user_id_for_session(pool: &SqlitePool, token: &str) -> Option<String> {
    let now = Utc::now().timestamp();
    sqlx::query_scalar::<_, String>("SELECT user_id FROM sessions WHERE token = ? AND expires_at > ?")
        .bind(token)
        .bind(now)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

/// Extractor that resolves the authenticated user's id from the session
/// cookie, rejecting the request with 401 if it's missing, unknown, or
/// expired.
pub struct AuthUser(pub String);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or((StatusCode::UNAUTHORIZED, "not logged in".to_string()))?;
        let user_id = user_id_for_session(&state.db, &token)
            .await
            .ok_or((StatusCode::UNAUTHORIZED, "session expired".to_string()))?;
        Ok(AuthUser(user_id))
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub password_hash: String,
}

#[derive(Debug, Serialize)]
pub struct UserDto {
    pub id: String,
    pub email: String,
}

impl From<UserRow> for UserDto {
    fn from(row: UserRow) -> Self {
        Self { id: row.id, email: row.email }
    }
}

pub async fn post_login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserDto>), (StatusCode, String)> {
    let row = sqlx::query_as::<_, UserRow>("SELECT id, email, password_hash FROM users WHERE email = ?")
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "invalid email or password".to_string()))?;

    if !verify_password(&req.password, &row.password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "invalid email or password".to_string()));
    }

    let token = create_session(&state.db, &row.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((jar.add(session_cookie(token)), Json(row.into())))
}

pub async fn post_logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> CookieJar {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let _ = delete_session(&state.db, cookie.value()).await;
    }
    jar.remove(Cookie::from(SESSION_COOKIE))
}

pub async fn get_me(
    AuthUser(user_id): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserDto>, StatusCode> {
    let row = sqlx::query_as::<_, UserRow>("SELECT id, email, password_hash FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(row.into()))
}
