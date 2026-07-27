use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::models::portfolio;
use crate::requests::RenamePortfolioForm;
use axum::extract::{Form, Path, State};
use axum::response::Redirect;
use uuid::Uuid;

pub async fn rename_portfolio(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<RenamePortfolioForm>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Portfolio name cannot be empty".into(),
        ));
    }
    portfolio::rename_portfolio(&state.db().await, portfolio_id, form.name.trim()).await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}
