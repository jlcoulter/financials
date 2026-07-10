use crate::AppState;
use crate::cookies::login_cookie;
use crate::cookies::logout_cookie;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::user;

use axum::extract::Form;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum_extra::extract::SignedCookieJar;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login(State(_state): State<AppState>) -> impl IntoResponse {
    layout(
        "Login",
        maud::html! {
            div class="auth-form" {
                form action="/login" hx-post="/login" hx-target="#error-box" method="post" {
                    label { "Username" input type="text" name="username"; }
                    label { "Password" input type="password" name="password"; }
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
    match user::get_user_by_username(state.db(), &form.username).await? {
        Some((user_id, hash)) => {
            let valid = bcrypt::verify(&form.password, &hash)?;
            if valid {
                let jar = jar.add(login_cookie(user_id));
                Ok((jar, [("HX-Redirect", "/dashboard")]).into_response())
            } else {
                Err(AppError::Unauthorized(
                    "Invalid username or password".to_string(),
                ))
            }
        }
        None => Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        )),
    }
}

pub async fn logout_post(jar: SignedCookieJar) -> impl IntoResponse {
    let jar = jar.add(logout_cookie());
    (jar, Redirect::to("/"))
}

pub async fn signup(State(_state): State<AppState>) -> impl IntoResponse {
    layout(
        "Sign up",
        maud::html! {
            div class="auth-form" {
                form action="/signup" hx-post="/signup" hx-target="#error-box" method="post" {
                    label { "Username" input type="text" name="username"; }
                    label { "Password" input type="password" name="password"; }
                    button type="submit" { "Sign up" }
                }
                div id="error-box" {}
            }
        },
        None,
    )
}

#[derive(Deserialize)]
pub struct SignupForm {
    username: String,
    password: String,
}

pub async fn signup_post(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(form): Form<SignupForm>,
) -> Result<axum::response::Response, AppError> {
    if form.username.trim().is_empty() {
        return Err(AppError::BadRequest("Username is required".to_string()));
    }
    if form.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    let hash = bcrypt::hash(&form.password, bcrypt::DEFAULT_COST)?;

    let user_id = user::create_user(state.db(), form.username.trim(), &hash).await?;
    let jar = jar.add(login_cookie(user_id));
    Ok((jar, [("HX-Redirect", "/dashboard")]).into_response())
}
