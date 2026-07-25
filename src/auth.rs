use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::cookies::login_cookie;
use crate::cookies::logout_cookie;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::user;

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
    State(state): State<AppState>,
    Query(params): Query<FlashParam>,
) -> Result<maud::Markup, AppError> {
    let flash = params.flash.as_deref();

    // Check if the admin user needs to set a password (first boot).
    let uid = *state.admin_user_id.read().unwrap();
    let needs_password = user::password_change_required(&state.db().await, uid)
        .await
        .unwrap_or(false);

    if needs_password {
        return Ok(layout(
            "Set Your Password",
            maud::html! {
                div class="auth-form" {
                    h2 { "Set Your Password" }
                    p { "Welcome! Please set your admin password to continue." }
                    form action="/setup-password" hx-post="/setup-password"
                         hx-target="#error-box" method="post" {
                        label { "New Password"
                            input type="password" name="new_password" autofocus {};
                        }
                        label { "Confirm New Password"
                            input type="password" name="confirm_password" {};
                        }
                        button type="submit" { "Set Password" }
                    }
                    div id="error-box" {}
                }
            },
            None,
        ));
    }

    Ok(layout(
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
    ))
}

pub async fn login_post(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(form): Form<LoginForm>,
) -> Result<axum::response::Response, AppError> {
    let uid = *state.admin_user_id.read().unwrap();

    // If the user hasn't set a password yet, accept any input and redirect
    // them to the change-password page.
    if user::password_change_required(&state.db().await, uid).await? {
        let jar = jar.add(login_cookie(uid, state.secure_cookies));
        return Ok((jar, [("HX-Redirect", "/change-password")]).into_response());
    }

    let valid = bcrypt::verify(&form.password, &state.admin_password_hash.read().unwrap())?;
    if valid {
        let jar = jar.add(login_cookie(uid, state.secure_cookies));
        Ok((jar, [("HX-Redirect", "/dashboard")]).into_response())
    } else {
        Err(AppError::Unauthorized("Invalid password".to_string()))
    }
}

pub async fn logout_post(jar: SignedCookieJar) -> impl IntoResponse {
    let jar = jar.add(logout_cookie());
    (jar, Redirect::to("/"))
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    current_password: Option<String>,
    new_password: String,
    confirm_password: String,
}

pub async fn change_password_get(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> impl IntoResponse {
    let needs_password = user::password_change_required(&state.db().await, user.0)
        .await
        .unwrap_or(false);
    layout(
        "Change Password",
        maud::html! {
            div class="auth-form" {
                @if needs_password {
                    h2 { "Set Your Password" }
                    p { "Welcome! Please set your admin password to continue." }
                } @else {
                    h2 { "Change Your Password" }
                    p { "Update your login password." }
                }
                form action="/change-password" hx-post="/change-password"
                     hx-target="#error-box" method="post" {
                    @if !needs_password {
                        label { "Current Password"
                            input type="password" name="current_password" autofocus {};
                        }
                    }
                    label { "New Password"
                        input type="password" name="new_password" autofocus[needs_password] {};
                    }
                    label { "Confirm New Password"
                        input type="password" name="confirm_password" {};
                    }
                    button type="submit" { @if needs_password { "Set Password" } @else { "Change Password" } }
                }
                div id="error-box" {}
            }
        },
        Some(&user),
    )
}

pub async fn change_password_post(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<ChangePasswordForm>,
) -> Result<axum::response::Response, AppError> {
    // Validate new password
    if form.new_password.len() < 4 {
        return Err(AppError::BadRequest(
            "New password must be at least 4 characters".to_string(),
        ));
    }
    if form.new_password != form.confirm_password {
        return Err(AppError::BadRequest(
            "New passwords do not match".to_string(),
        ));
    }

    // If the user already has a password set, verify the current one
    if !user::password_change_required(&state.db().await, user.0).await? {
        let current = form.current_password.as_deref().unwrap_or("");
        let valid = bcrypt::verify(
            current.as_bytes(),
            &state.admin_password_hash.read().unwrap(),
        )?;
        if !valid {
            return Err(AppError::Unauthorized(
                "Current password is incorrect".to_string(),
            ));
        }
    }

    // Hash and save
    let new_hash = bcrypt::hash(&form.new_password, bcrypt::DEFAULT_COST)?;
    user::update_password(&state.db().await, user.0, &new_hash).await?;

    // Update the in-memory hash so subsequent logins use the new one
    *state.admin_password_hash.write().unwrap() = new_hash;

    Ok(([("HX-Redirect", "/settings?flash=password_changed")]).into_response())
}

/// First-boot password setup — no auth required (user has no password yet).
#[derive(Deserialize)]
pub struct SetupPasswordForm {
    new_password: String,
    confirm_password: String,
}

pub async fn setup_password_post(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(form): Form<SetupPasswordForm>,
) -> Result<axum::response::Response, AppError> {
    let uid = *state.admin_user_id.read().unwrap();

    // Verify this user actually needs a password set
    if !user::password_change_required(&state.db().await, uid).await? {
        return Err(AppError::BadRequest(
            "Password is already set. Please log in.".to_string(),
        ));
    }

    // Validate new password
    if form.new_password.len() < 4 {
        return Err(AppError::BadRequest(
            "New password must be at least 4 characters".to_string(),
        ));
    }
    if form.new_password != form.confirm_password {
        return Err(AppError::BadRequest(
            "New passwords do not match".to_string(),
        ));
    }

    // Hash and save
    let new_hash = bcrypt::hash(&form.new_password, bcrypt::DEFAULT_COST)?;
    user::update_password(&state.db().await, uid, &new_hash).await?;

    // Update the in-memory hash so subsequent logins use the new one
    *state.admin_password_hash.write().unwrap() = new_hash;

    // Log the user in
    let jar = jar.add(login_cookie(uid, state.secure_cookies));

    Ok((jar, [("HX-Redirect", "/dashboard")]).into_response())
}
