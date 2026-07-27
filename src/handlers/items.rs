use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::models::portfolio;
use crate::requests::{AddItemForm, ChangeTypeForm, DeleteItemForm, MoveItemQuery};
use axum::extract::{Form, Path, Query, State};
use axum::response::Redirect;
use uuid::Uuid;

pub async fn add_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<AddItemForm>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    portfolio::create_wealth_item(&state.db().await, portfolio_id, &form.name, &form.item_type)
        .await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

pub async fn move_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(query): Query<MoveItemQuery>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    portfolio::move_wealth_item(
        &state.db().await,
        portfolio_id,
        query.item_id,
        &query.direction,
    )
    .await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

pub async fn save_item_name(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let item_id_str = form
        .get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = Uuid::parse_str(item_id_str)?;
    let name = form
        .get("name")
        .ok_or_else(|| AppError::BadRequest("Missing name".into()))?;

    if name.trim().is_empty() {
        return Err(AppError::BadRequest("Item name cannot be empty".into()));
    }

    portfolio::rename_wealth_item(&state.db().await, item_id, name.trim()).await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

pub async fn change_item_type(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<ChangeTypeForm>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let valid_types = ["asset", "cash", "debt", "investment"];
    if !valid_types.contains(&form.item_type.as_str()) {
        return Err(AppError::BadRequest("Invalid item type".into()));
    }
    portfolio::change_wealth_item_type(&state.db().await, form.item_id, &form.item_type).await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

pub async fn delete_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<DeleteItemForm>,
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    portfolio::delete_wealth_item(&state.db().await, form.item_id).await?;
    Ok(Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}
