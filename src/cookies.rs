use crate::AppState;
use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::extract::OptionalFromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum_extra::extract::SignedCookieJar;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::Key;
use uuid::Uuid;

pub struct LoggedInUser(pub Uuid);

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.key.clone()
    }
}

impl FromRequestParts<AppState> for LoggedInUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar: SignedCookieJar<Key> = SignedCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        jar.get("user_id")
            .and_then(|c| Uuid::parse_str(c.value()).map(LoggedInUser).ok())
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

impl OptionalFromRequestParts<AppState> for LoggedInUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        let jar: SignedCookieJar<Key> = SignedCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(jar
            .get("user_id")
            .and_then(|c| Uuid::parse_str(c.value()).map(LoggedInUser).ok()))
    }
}

pub fn login_cookie(user_id: Uuid) -> Cookie<'static> {
    Cookie::build(("user_id", user_id.to_string()))
        .http_only(true)
        .path("/")
        .build()
}

pub fn logout_cookie() -> Cookie<'static> {
    Cookie::build(("user_id", ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
