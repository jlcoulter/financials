use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::csv_import;
use crate::models::reconcile::{self, OutgoingTxn, ReconciledTxn};
use crate::requests::{
    AddTxnForm, CreateSessionForm, RenameSessionForm, SortQuery, UnlinkForm, UnlinkReconciledForm,
};
use crate::utils;
use axum::extract::{Form, Multipart, Path, State};
use axum::response::Redirect;
use chrono::NaiveDate;
use uuid::Uuid;

// ── Session CRUD ──

pub async fn reconcile_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<CreateSessionForm>,
) -> Result<Redirect, AppError> {
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Session name is required".into()));
    }
    let id = reconcile::create_session(&state.db().await, user.0, form.name.trim()).await?;
    Ok(Redirect::to(&format!("/reconcile/{}", id)))
}

pub async fn reconcile_delete(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    reconcile::delete_session(&state.db().await, session_id).await?;
    Ok(Redirect::to("/reconcile"))
}

// ── Add transactions ──

pub async fn add_outgoing(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<AddTxnForm>,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&form.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let cents = utils::parse_dollars(&form.amount)
        .map_err(AppError::BadRequest)?
        .abs();
    let vendor = form
        .vendor
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    reconcile::add_outgoing(
        &state.db().await,
        session_id,
        date,
        cents,
        &vendor,
        &std::collections::HashMap::new(),
    )
    .await?;
    Ok(Redirect::to(&format!("/reconcile/{}", session_id)))
}

pub async fn add_reconciled(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<AddTxnForm>,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&form.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let cents = utils::parse_dollars(&form.amount)
        .map_err(AppError::BadRequest)?
        .abs();
    let vendor = form
        .vendor
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    reconcile::add_reconciled(
        &state.db().await,
        session_id,
        date,
        cents,
        &vendor,
        &std::collections::HashMap::new(),
    )
    .await?;
    Ok(Redirect::to(&format!("/reconcile/{}", session_id)))
}

// ── Link / Unlink ──

pub async fn link_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut outgoing_id: Option<Uuid> = None;
    let mut reconciled_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            match key {
                "outgoing_id" => {
                    outgoing_id = Some(
                        Uuid::parse_str(val)
                            .map_err(|_| AppError::BadRequest("Invalid outgoing ID".into()))?,
                    );
                }
                "reconciled_ids" => {
                    let id = Uuid::parse_str(val)
                        .map_err(|_| AppError::BadRequest("Invalid reconciled ID".into()))?;
                    reconciled_ids.push(id);
                }
                _ => {}
            }
        }
    }
    let outgoing_id =
        outgoing_id.ok_or_else(|| AppError::BadRequest("No outgoing selected".into()))?;
    if reconciled_ids.is_empty() {
        return Err(AppError::BadRequest(
            "No reconciled transaction selected".into(),
        ));
    }
    for reconciled_id in reconciled_ids {
        reconcile::link_transactions(&state.db().await, outgoing_id, reconciled_id).await?;
    }
    render_sections(
        session_id,
        reconcile::SortOrder::default(),
        &state.db().await,
    )
    .await
}

pub async fn unlink_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<UnlinkForm>,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let outgoing_id = Uuid::parse_str(&form.outgoing_id)
        .map_err(|_| AppError::BadRequest("Invalid outgoing ID".into()))?;
    // Find and remove all match_links for this outgoing
    let matches = reconcile::list_matches(&state.db().await, session_id).await?;
    for m in matches.iter().filter(|m| m.outgoing_id == outgoing_id) {
        reconcile::unlink_transaction(&state.db().await, m.match_id).await?;
    }
    render_sections(
        session_id,
        reconcile::SortOrder::default(),
        &state.db().await,
    )
    .await
}

pub async fn unlink_reconciled_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<UnlinkReconciledForm>,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let reconciled_id = Uuid::parse_str(&form.reconciled_id)
        .map_err(|_| AppError::BadRequest("Invalid reconciled ID".into()))?;
    let matches = reconcile::list_matches(&state.db().await, session_id).await?;
    for m in matches.iter().filter(|m| m.reconciled_id == reconciled_id) {
        reconcile::unlink_transaction(&state.db().await, m.match_id).await?;
    }
    render_sections(
        session_id,
        reconcile::SortOrder::default(),
        &state.db().await,
    )
    .await
}

// ── Ignore / Unignore ──

pub async fn ignore_outgoing(
    Path((session_id, txn_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<SortQuery>,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    reconcile::ignore_outgoing(&state.db().await, txn_id).await?;
    render_sections(session_id, form.sort.unwrap_or_default(), &state.db().await).await
}

pub async fn ignore_reconciled(
    Path((session_id, txn_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<SortQuery>,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    reconcile::ignore_reconciled(&state.db().await, txn_id).await?;
    render_sections(session_id, form.sort.unwrap_or_default(), &state.db().await).await
}

pub async fn unignore_outgoing(
    Path((session_id, txn_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    reconcile::unignore_outgoing(&state.db().await, txn_id).await?;
    render_sections(
        session_id,
        reconcile::SortOrder::default(),
        &state.db().await,
    )
    .await
}

pub async fn unignore_reconciled(
    Path((session_id, txn_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    reconcile::unignore_reconciled(&state.db().await, txn_id).await?;
    render_sections(
        session_id,
        reconcile::SortOrder::default(),
        &state.db().await,
    )
    .await
}

// ── Auto-match ──

pub async fn auto_match(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    render_proposals_page(session_id, state, user, &[]).await
}

async fn render_proposals_page(
    session_id: Uuid,
    state: AppState,
    user: LoggedInUser,
    skip_ids: &[Uuid],
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let proposals = reconcile::auto_match(&state.db().await, session_id, skip_ids).await?;
    let (_, name) = reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let outgoing = reconcile::list_outgoing(
        &state.db().await,
        session_id,
        reconcile::SortOrder::default(),
    )
    .await?;
    let reconciled = reconcile::list_reconciled(
        &state.db().await,
        session_id,
        reconcile::SortOrder::default(),
    )
    .await?;

    Ok(layout(
        &format!("Reconcile — {}", name),
        maud::html! {
            a href="/reconcile" { "← Back" }
            form class="portfolio-name-form" method="post" action=(format!("/reconcile/{}/rename", session_id)) {
                input type="text" name="name" value=(name)
                       class="portfolio-name-input"
                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
            }

            h2 { "Auto-Match Proposals" }

            @if proposals.is_empty() && skip_ids.is_empty() {
                p { "No matches found." }
                a href=(format!("/reconcile/{}", session_id)) { "← Back to reconcile" }
            } @else {
                @if !proposals.is_empty() {
                    p { (format!("Found {} proposed match(es). Review and confirm or reject each.", proposals.len())) }

                    form method="post" action=(format!("/reconcile/{}/confirm-all", session_id)) {
                        @for sid in skip_ids {
                            input type="hidden" name="skip_ids" value=(sid) {}
                        }
                        button type="submit" class="btn" { "Confirm All" }
                        " "
                        a href=(format!("/reconcile/{}", session_id)) class="btn btn-ghost" { "Cancel" }
                    }

                    div class="reconcile-grid" style="margin-top:1rem" {
                        div class="reconcile-grid-header" { "Outgoing" }
                        div class="reconcile-grid-header" { "Reconciled" }

                        @for p in &proposals {
                            @if let Some(o) = outgoing.iter().find(|x| x.txn_id == p.outgoing_id) {
                                @let row_span = p.reconciled_ids.len().max(1);
                                @let is_exact = p.reconciled_ids.len() == 1;
                                @let txn_class = if is_exact { "reconcile-txn reconcile-txn--exact-match" } else { "reconcile-txn reconcile-txn--proposed" };
                                div class=(txn_class) style=(format!("grid-row: span {}", row_span)) {
                                    div class="txn-row" {
                                        span class="txn-date" { (utils::format_date(o.date)) }
                                        @if !o.vendor.is_empty() {
                                            span class="txn-vendor" { (o.vendor) }
                                        }
                                        span class="txn-amount" { (utils::format_cents(o.amount)) }
                                        @if is_exact {
                                            span class="txn-confidence" { "Exact match" }
                                        }
                                        form method="post" action=(format!("/reconcile/{}/confirm", session_id)) class="txn-unlink-form" style="display:inline" {
                                            input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                            @for rid in &p.reconciled_ids {
                                                input type="hidden" name="reconciled_ids" value=(rid) {}
                                            }
                                            @for sid in skip_ids {
                                                input type="hidden" name="skip_ids" value=(sid) {}
                                            }
                                            button type="submit" class="btn btn-sm" { "Confirm" }
                                        }
                                        form method="post" action=(format!("/reconcile/{}/reject", session_id)) class="txn-unlink-form" style="display:inline" {
                                            input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                            @for sid in skip_ids {
                                                input type="hidden" name="skip_ids" value=(sid) {}
                                            }
                                            button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Reject" }
                                        }
                                    }
                                }
                                @for rid in &p.reconciled_ids {
                                    @if let Some(r) = reconciled.iter().find(|x| x.txn_id == *rid) {
                                        div class=(txn_class) {
                                            div class="txn-row" {
                                                span class="txn-date" { (utils::format_date(r.date)) }
                                                @if !r.vendor.is_empty() {
                                                    span class="txn-vendor" { (r.vendor) }
                                                }
                                                span class="txn-amount" { (utils::format_cents(r.amount)) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                @if !skip_ids.is_empty() {
                    @let rejected_outgoing: Vec<&reconcile::OutgoingTxn> = outgoing.iter().filter(|o| skip_ids.contains(&o.txn_id)).collect();
                    @if !rejected_outgoing.is_empty() {
                        h3 style="margin-top:2rem" { "Rejected Proposals" }
                        p { "These proposals were rejected. Click Undo to return them to the proposals list." }
                        div class="reconcile-grid" {
                            div class="reconcile-grid-header" { "Outgoing" }
                            div class="reconcile-grid-header" { "Reconciled" }
                            @for o in &rejected_outgoing {
                                div class="reconcile-txn reconcile-txn--rejected" {
                                    div class="txn-row" {
                                        span class="txn-date" { (utils::format_date(o.date)) }
                                        @if !o.vendor.is_empty() {
                                            span class="txn-vendor" { (o.vendor) }
                                        }
                                        span class="txn-amount" { (utils::format_cents(o.amount)) }
                                        form method="post" action=(format!("/reconcile/{}/undo-reject/{}", session_id, o.txn_id)) class="txn-unlink-form" style="display:inline" {
                                            @for sid in skip_ids {
                                                @if *sid != o.txn_id {
                                                    input type="hidden" name="skip_ids" value=(sid) {}
                                                }
                                            }
                                            button type="submit" class="btn btn-sm" { "Undo" }
                                        }
                                    }
                                }
                                div class="reconcile-txn reconcile-txn--rejected" {
                                    div class="txn-row" {
                                        span { "Rejected" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        Some(&user),
    ))
}

pub async fn confirm_proposal(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut outgoing_id: Option<Uuid> = None;
    let mut reconciled_ids: Vec<Uuid> = Vec::new();
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = val.to_string();
            match key.as_str() {
                "outgoing_id" => {
                    outgoing_id = val.parse().ok();
                }
                "reconciled_ids" => {
                    if let Ok(id) = val.parse() {
                        reconciled_ids.push(id);
                    }
                }
                "skip_ids" => {
                    if let Ok(id) = val.parse() {
                        skip_ids.push(id);
                    }
                }
                _ => {}
            }
        }
    }
    // Apply this match
    if let Some(oid) = outgoing_id {
        for rid in &reconciled_ids {
            reconcile::link_transactions(&state.db().await, oid, *rid).await?;
        }
        skip_ids.push(oid);
    }
    // Re-render remaining proposals
    render_proposals_page(session_id, state, user, &skip_ids).await
}

pub async fn confirm_all_proposals(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=')
            && key == "skip_ids"
            && let Ok(id) = val.parse()
        {
            skip_ids.push(id);
        }
    }
    let proposals = reconcile::auto_match(&state.db().await, session_id, &skip_ids).await?;
    for p in &proposals {
        for rid in &p.reconciled_ids {
            reconcile::link_transactions(&state.db().await, p.outgoing_id, *rid).await?;
        }
    }
    Ok(Redirect::to(&format!("/reconcile/{}", session_id)))
}

pub async fn reject_proposal(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut rejected_id: Option<Uuid> = None;
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = val.to_string();
            match key.as_str() {
                "outgoing_id" => {
                    if let Ok(id) = val.parse() {
                        rejected_id = Some(id);
                    }
                }
                "skip_ids" => {
                    if let Ok(id) = val.parse() {
                        skip_ids.push(id);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(id) = rejected_id {
        skip_ids.push(id);
    }
    render_proposals_page(session_id, state, user, &skip_ids).await
}

pub async fn undo_reject(
    Path((session_id, outgoing_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=')
            && key == "skip_ids"
            && let Ok(id) = val.parse()
            && id != outgoing_id
        {
            skip_ids.push(id);
        }
    }
    render_proposals_page(session_id, state, user, &skip_ids).await
}

// ── Rename session ──

pub async fn rename_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<RenameSessionForm>,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Session name cannot be empty".into()));
    }
    sqlx::query("UPDATE reconcile_sessions SET name = ? WHERE session_id = ?")
        .bind(form.name.trim())
        .bind(session_id.to_string())
        .execute(&state.db().await)
        .await?;
    Ok(Redirect::to(&format!("/reconcile/{}", session_id)))
}

// ── CSV upload ──

pub async fn upload_outgoing_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    multipart: Multipart,
) -> Result<maud::Markup, AppError> {
    upload_csv(session_id, state, user, multipart, "outgoing").await
}

pub async fn upload_reconciled_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    multipart: Multipart,
) -> Result<maud::Markup, AppError> {
    upload_csv(session_id, state, user, multipart, "reconciled").await
}

async fn upload_csv(
    session_id: Uuid,
    state: AppState,
    user: LoggedInUser,
    mut multipart: Multipart,
    kind: &str,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let (_, name) = reconcile::get_session(&state.db().await, session_id, user.0).await?;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?
    {
        if field.name() == Some("csv_file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?;
            let raw = String::from_utf8(bytes.to_vec())
                .map_err(|_| AppError::BadRequest("CSV must be UTF-8".into()))?;
            let analysis = csv_import::analyze_csv(&raw)?;

            // Save CSV to temp file for confirm step
            let tmp_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
            let tmp_path = format!("/tmp/financials_csv_{}_{}.csv", session_id, tmp_id);
            std::fs::write(&tmp_path, &raw)
                .map_err(|e| AppError::BadRequest(format!("Failed to save CSV: {}", e)))?;

            let num_cols = analysis.preview_rows.first().map(|r| r.len()).unwrap_or(0);
            let col_options: Vec<String> = (0..num_cols)
                .map(|i| {
                    let example = analysis
                        .preview_rows
                        .first()
                        .and_then(|row| row.get(i))
                        .filter(|v| !v.is_empty())
                        .map(|v| format!(" > {}", v))
                        .unwrap_or_default();
                    format!("Column {}{}", i + 1, example)
                })
                .collect();

            // Grab a sample date from the first preview row to show format examples
            let date_examples: Vec<(&'static str, String)> = analysis
                .preview_rows
                .first()
                .and_then(|row| {
                    let sample = row.get(analysis.detected.date_col)?;
                    if sample.is_empty() {
                        return None;
                    }
                    crate::views::portfolio::date_format_examples(
                        sample,
                        &analysis.detected.date_format,
                    )
                })
                .unwrap_or_default();

            return Ok(layout(
                &format!("Import CSV — {}", name),
                maud::html! {
                    a href=(format!("/reconcile/{}", session_id)) { "← Back" }

                    h2 { "Import " (if kind == "outgoing" { "Outgoing" } else { "Reconciled" }) " Transactions" }

                    p { (format!("Detected {} rows. Review column mapping below and adjust if needed.", analysis.total_rows)) }

                    div class="csv-preview" {
                        h3 { "Preview (first 5 rows)" }
                        table class="csv-preview-table" {
                            thead {
                                tr class="csv-col-numbers" {
                                    @for i in 0..num_cols {
                                        th { (i + 1) }
                                    }
                                }
                                tr {
                                    @if !analysis.headers.is_empty() {
                                        @for h in &analysis.headers {
                                            th { (h) }
                                        }
                                    } @else {
                                        @for i in 0..num_cols {
                                            th { (format!("Col {}", i + 1)) }
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for row in &analysis.preview_rows {
                                    tr {
                                        @for cell in row {
                                            td { (cell) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    form method="post" action=(format!("/reconcile/{}/{}-csv/confirm", session_id, kind)) {
                        input type="hidden" name="tmp_id" value=(tmp_id) {}

                        div class="csv-mapping" {
                            h3 { "Column Mapping" }

                            label { "Date column" }
                            select name="date_col" {
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[i == analysis.detected.date_col] { (label) }
                                }
                            }

                            label { "Amount column" }
                            select name="amount_col" {
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[i == analysis.detected.amount_col] { (label) }
                                }
                            }

                            label { "Vendor/description column" }
                            select name="vendor_col" {
                                option value="" { "— None —" }
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[analysis.detected.vendor_col == Some(i)] { (label) }
                                }
                            }

                            label { "Date format" }
                            select name="date_format" {
                                @for (fmt, example) in &date_examples {
                                    @let selected = if *fmt == analysis.detected.date_format { " selected" } else { "" };
                                    option value=(fmt) selected[selected == " selected"] { (format!("{}  →  {}", fmt, example)) }
                                }
                            }
                        }

                        button type="submit" class="btn" { "Import" }
                        " "
                        a href=(format!("/reconcile/{}", session_id)) class="btn btn-ghost" { "Cancel" }
                    }
                },
                Some(&user),
            ));
        }
    }
    Err(AppError::BadRequest("No CSV file provided".into()))
}

pub async fn confirm_outgoing_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<Redirect, AppError> {
    confirm_csv_import(session_id, state, user, body, "outgoing").await
}

pub async fn confirm_reconciled_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<Redirect, AppError> {
    confirm_csv_import(session_id, state, user, body, "reconciled").await
}

async fn confirm_csv_import(
    session_id: Uuid,
    state: AppState,
    user: LoggedInUser,
    body: axum::body::Bytes,
    kind: &str,
) -> Result<Redirect, AppError> {
    reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut tmp_id = String::new();
    let mut date_col: Option<usize> = None;
    let mut amount_col: Option<usize> = None;
    let mut vendor_col: Option<usize> = None;
    let mut date_format = String::new();

    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = urldecode(val);
            match key.as_str() {
                "tmp_id" => tmp_id = val,
                "date_col" => date_col = val.parse().ok(),
                "amount_col" => amount_col = val.parse().ok(),
                "vendor_col" => {
                    if !val.is_empty() {
                        vendor_col = val.parse().ok();
                    }
                }
                "date_format" => date_format = val,
                _ => {}
            }
        }
    }

    let date_col = date_col.ok_or_else(|| AppError::BadRequest("Missing date_col".into()))?;
    let amount_col = amount_col.ok_or_else(|| AppError::BadRequest("Missing amount_col".into()))?;
    if date_format.is_empty() {
        date_format = "%Y-%m-%d".to_string();
    }

    let tmp_path = format!("/tmp/financials_csv_{}_{}.csv", session_id, tmp_id);
    let raw = std::fs::read_to_string(&tmp_path)
        .map_err(|e| AppError::BadRequest(format!("CSV file not found: {}", e)))?;
    let _ = std::fs::remove_file(&tmp_path); // Clean up

    let mapping = csv_import::ColumnMapping {
        date_col,
        amount_col,
        vendor_col,
        date_format,
    };
    let rows = csv_import::parse_csv_with_mapping(&raw, &mapping)?;

    if kind == "outgoing" {
        reconcile::bulk_add_outgoing(&state.db().await, session_id, &rows).await?;
    } else {
        reconcile::bulk_add_reconciled(&state.db().await, session_id, &rows).await?;
    }

    Ok(Redirect::to(&format!("/reconcile/{}", session_id)))
}

// ── Render sections (private) ──

async fn render_sections(
    session_id: Uuid,
    sort: reconcile::SortOrder,
    pool: &sqlx::SqlitePool,
) -> Result<maud::Markup, AppError> {
    let outgoing = reconcile::list_outgoing(pool, session_id, sort).await?;
    let reconciled = reconcile::list_reconciled(pool, session_id, sort).await?;
    let matches = reconcile::list_matches(pool, session_id).await?;
    let ignored_outgoing = reconcile::list_ignored_outgoing(pool, session_id, sort).await?;
    let ignored_reconciled = reconcile::list_ignored_reconciled(pool, session_id, sort).await?;

    let mut match_map: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    for m in &matches {
        match_map
            .entry(m.outgoing_id)
            .or_default()
            .push(m.reconciled_id);
    }

    let unmatched_outgoing: Vec<&OutgoingTxn> = outgoing.iter().filter(|o| !o.matched).collect();
    let unmatched_reconciled: Vec<&ReconciledTxn> =
        reconciled.iter().filter(|r| !r.matched).collect();
    let unmatched_max = unmatched_outgoing.len().max(unmatched_reconciled.len());
    let matched_outgoing: Vec<&OutgoingTxn> = outgoing.iter().filter(|o| o.matched).collect();
    let ignored_max = ignored_outgoing.len().max(ignored_reconciled.len());

    Ok(crate::views::reconcile::render_reconcile_sections(
        session_id,
        sort,
        &unmatched_outgoing,
        &unmatched_reconciled,
        unmatched_max,
        &matched_outgoing,
        &match_map,
        &reconciled,
        &ignored_outgoing,
        &ignored_reconciled,
        ignored_max,
    ))
}

// ── URL decode helper ──

fn urldecode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urldecode_percent_encoding() {
        assert_eq!(urldecode("hello%20world"), "hello world");
    }

    #[test]
    fn urldecode_plus() {
        assert_eq!(urldecode("hello+world"), "hello world");
    }

    #[test]
    fn urldecode_ampersand() {
        assert_eq!(urldecode("a%26b"), "a&b");
    }

    #[test]
    fn urldecode_plain_string() {
        assert_eq!(urldecode("hello"), "hello");
    }

    #[test]
    fn urldecode_multi_byte() {
        // The urldecode function handles one byte at a time, so multi-byte
        // UTF-8 chars produce multiple decoded chars. %C3%A9 → two chars (Ã©)
        let result = urldecode("%C3%A9");
        assert!(result.contains("Ã"));
    }
}
