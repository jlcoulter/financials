use crate::AppState;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Redirect;

pub async fn hello() -> impl IntoResponse {
    Redirect::to("/login")
}

pub async fn time(State(_state): State<AppState>) -> impl IntoResponse {
    maud::html! { p { "Time: " (chrono::Local::now().format("%H:%M:%S")) } }
}

pub async fn not_found(State(_state): State<AppState>) -> impl IntoResponse {
    crate::layout::layout(
        "Not Found",
        maud::html! {
            h1 { "404" }
            p { "The page you're looking for doesn't exist." }
            a href="/" { "Go home" }
        },
        None,
    )
}
