use crate::AppState;
use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::extract::OptionalFromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum_extra::extract::SignedCookieJar;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::Key;

pub struct LoggedInUser(pub String);

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
        jar.get("username")
            .map(|c| LoggedInUser(c.value().to_string()))
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
            .get("username")
            .map(|c| LoggedInUser(c.value().to_string())))
    }
}

pub fn login_cookie(username: &str) -> Cookie<'static> {
    Cookie::build(("username", username.to_string()))
        .http_only(true)
        .path("/")
        .build()
}

pub fn logout_cookie() -> Cookie<'static> {
    Cookie::build(("username", ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
