use crate::AppState;
use crate::cookies::login_cookie;
use crate::cookies::logout_cookie;
use crate::error::AppError;
use crate::layout::layout;

use axum::extract::{Form, Query, State};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum_extra::extract::SignedCookieJar;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

#[derive(Deserialize)]
pub struct FlashParam {
    flash: Option<String>,
}

pub async fn login(
    State(_state): State<AppState>,
    Query(params): Query<FlashParam>,
) -> impl IntoResponse {
    let flash = params.flash.as_deref();
    layout(
        "Login",
        maud::html! {
            div class="auth-form" {
                @if let Some(msg) = flash {
                    @if msg == "restored" {
                        div class="flash flash-success" { "Database restored from backup. Please log in again." }
                    } @else if msg == "restore_failed" {
                        div class="flash flash-error" { "Restore failed — check server logs for details." }
                    }
                }
                form action="/login" hx-post="/login" hx-target="#error-box" method="post" {
                    label { "Password"
                        input type="password" name="password" autofocus {};
                    }
                    button type="submit" { "Login" }
                }
                div id="error-box" {}
            }
        },
        None,
    )
}

pub async fn login_post(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(form): Form<LoginForm>,
) -> Result<axum::response::Response, AppError> {
    let valid = bcrypt::verify(&form.password, &state.admin_password_hash)?;
    if valid {
        let uid = *state.admin_user_id.read().unwrap();
        let jar = jar.add(login_cookie(uid));
        Ok((jar, [("HX-Redirect", "/dashboard")]).into_response())
    } else {
        Err(AppError::Unauthorized("Invalid password".to_string()))
    }
}

pub async fn logout_post(jar: SignedCookieJar) -> impl IntoResponse {
    let jar = jar.add(logout_cookie());
    (jar, Redirect::to("/"))
}
