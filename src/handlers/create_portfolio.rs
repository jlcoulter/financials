use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::models::portfolio;
use crate::requests::CreatePortfolioForm;
use axum::extract::{Form, State};
use axum::response::Redirect;

pub async fn create_portfolio(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<CreatePortfolioForm>,
) -> Result<Redirect, AppError> {
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Portfolio name is required".into()));
    }
    portfolio::create_portfolio(&state.db().await, user.0, form.name.trim()).await?;
    Ok(Redirect::to("/portfolios"))
}
