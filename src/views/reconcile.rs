use crate::error::AppError;
use crate::layout::layout;
use crate::models::reconcile::{self, OutgoingTxn, ReconciledTxn};
use crate::utils;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

pub async fn reconcile_list(
    State(state): State<crate::AppState>,
    user: crate::cookies::LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let sessions = reconcile::list_sessions(&state.db().await, user.0).await?;
    Ok(layout(
        "Reconcile",
        maud::html! {
            h2 { "Reconcile" }
            details class="add-item-details" {
                summary { "+ New Reconcile Session" }
                form method="post" action="/reconcile" class="add-item-form" {
                    label { "Name"
                        input type="text" name="name" required {}
                    }
                    button type="submit" { "Create" }
                }
            }
            @if sessions.is_empty() {
                p { "No reconcile sessions yet. Create one to start matching transactions." }
            } @else {
                div class="portfolio-list" {
                    @for (id, name) in &sessions {
                        div class="portfolio-row" {
                            div class="portfolio-info" {
                                h3 { (name) }
                            }
                            div class="portfolio-actions" {
                                a href=(format!("/reconcile/{}", id)) class="btn-view" { "View" }
                                form method="post" action=(format!("/reconcile/{}/delete", id))
                                     style="display:inline" {
                                    button type="submit" class="btn-ghost" style="margin-left:0.5rem"
                                            onclick="return confirm('Delete this session and all its data?')" { "Delete" }
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

pub async fn reconcile_detail(
    Path(session_id): Path<Uuid>,
    State(state): State<crate::AppState>,
    user: crate::cookies::LoggedInUser,
    Query(query): Query<crate::requests::SortQuery>,
) -> Result<maud::Markup, AppError> {
    let sort = query.sort.unwrap_or_default();
    let (_, name) = reconcile::get_session(&state.db().await, session_id, user.0).await?;
    let outgoing = reconcile::list_outgoing(&state.db().await, session_id, sort).await?;
    let reconciled = reconcile::list_reconciled(&state.db().await, session_id, sort).await?;
    let matches = reconcile::list_matches(&state.db().await, session_id).await?;
    let ignored_outgoing =
        reconcile::list_ignored_outgoing(&state.db().await, session_id, sort).await?;
    let ignored_reconciled =
        reconcile::list_ignored_reconciled(&state.db().await, session_id, sort).await?;

    // Build lookup: outgoing_id -> list of reconciled_ids
    let mut match_map: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    let mut reverse_map: std::collections::HashMap<Uuid, Vec<(Uuid, Uuid)>> =
        std::collections::HashMap::new();
    for m in &matches {
        match_map
            .entry(m.outgoing_id)
            .or_default()
            .push(m.reconciled_id);
        reverse_map
            .entry(m.reconciled_id)
            .or_default()
            .push((m.match_id, m.outgoing_id));
    }

    let unmatched_outgoing: Vec<&OutgoingTxn> = outgoing.iter().filter(|o| !o.matched).collect();
    let unmatched_reconciled: Vec<&ReconciledTxn> =
        reconciled.iter().filter(|r| !r.matched).collect();
    let unmatched_max = unmatched_outgoing.len().max(unmatched_reconciled.len());

    let matched_outgoing: Vec<&OutgoingTxn> = outgoing.iter().filter(|o| o.matched).collect();
    let ignored_max = ignored_outgoing.len().max(ignored_reconciled.len());

    Ok(layout(
        &format!("Reconcile — {}", name),
        maud::html! {
            a href="/reconcile" { "← Back" }
            form class="portfolio-name-form" method="post" action=(format!("/reconcile/{}/rename", session_id)) {
                input type="text" name="name" value=(name)
                       class="portfolio-name-input"
                       onblur="this.closest('form').requestSubmit()"
                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
            }

            form id="reconcile-match-form" method="post" action=(format!("/reconcile/{}/link", session_id))
                hx-post=(format!("/reconcile/{}/link", session_id))
                hx-target="#reconcile-sections"
                hx-swap="morphdom" {}

            // ── Toolbar ──
            div class="reconcile-toolbar" {
                details class="add-item-details" {
                    summary { "+ Add Outgoing" }
                    form method="post" action=(format!("/reconcile/{}/outgoing", session_id)) class="add-item-form reconcile-add-form" {
                        label { "Date"
                            input type="text" name="date" placeholder="YYYY-MM-DD" required {}
                        }
                        label { "Amount"
                            input type="number" step="0.01" name="amount" placeholder="0.00" required {}
                        }
                        label { "Vendor"
                            input type="text" name="vendor" {}
                        }
                        button type="submit" { "Add" }
                    }
                }
                details class="add-item-details" {
                    summary { "+ Add Reconciled" }
                    form method="post" action=(format!("/reconcile/{}/reconciled", session_id)) class="add-item-form reconcile-add-form" {
                        label { "Date"
                            input type="text" name="date" placeholder="YYYY-MM-DD" required {}
                        }
                        label { "Amount"
                            input type="number" step="0.01" name="amount" placeholder="0.00" required {}
                        }
                        label { "Vendor"
                            input type="text" name="vendor" {}
                        }
                        button type="submit" { "Add" }
                    }
                }
                details class="add-item-details" {
                    summary { "↑ Upload CSV" }
                    div class="csv-dropzones" {
                        div class="dropzone" id=(format!("dz-outgoing-{}", session_id))
                            ondragover=(format!("event.preventDefault(); document.getElementById('dz-outgoing-{}').classList.add('dropzone--dragover')", session_id))
                            ondragleave=(format!("document.getElementById('dz-outgoing-{}').classList.remove('dropzone--dragover')", session_id))
                            ondrop=(format!("event.preventDefault(); document.getElementById('dz-outgoing-{}').classList.remove('dropzone--dragover'); var f=event.dataTransfer.files[0]; if(f){{document.getElementById('file-outgoing-{}').files=f; document.getElementById('file-outgoing-{}').closest('form').requestSubmit()}}", session_id, session_id, session_id))
                            onclick=(format!("document.getElementById('file-outgoing-{}').click()", session_id)) {
                            div class="dropzone-label" { "Outgoing CSV" }
                            div class="dropzone-hint" { "Drop file here or click to browse" }
                        }
                        div class="dropzone" id=(format!("dz-reconciled-{}", session_id))
                            ondragover=(format!("event.preventDefault(); document.getElementById('dz-reconciled-{}').classList.add('dropzone--dragover')", session_id))
                            ondragleave=(format!("document.getElementById('dz-reconciled-{}').classList.remove('dropzone--dragover')", session_id))
                            ondrop=(format!("event.preventDefault(); document.getElementById('dz-reconciled-{}').classList.remove('dropzone--dragover'); var f=event.dataTransfer.files[0]; if(f){{document.getElementById('file-reconciled-{}').files=f; document.getElementById('file-reconciled-{}').closest('form').requestSubmit()}}", session_id, session_id, session_id))
                            onclick=(format!("document.getElementById('file-reconciled-{}').click()", session_id)) {
                            div class="dropzone-label" { "Reconciled CSV" }
                            div class="dropzone-hint" { "Drop file here or click to browse" }
                        }
                    }
                    form method="post" action=(format!("/reconcile/{}/outgoing/csv", session_id))
                          enctype="multipart/form-data" style="display:none" {
                        input type="file" name="csv_file" accept=".csv"
                               id=(format!("file-outgoing-{}", session_id))
                               onchange="if(this.files.length){this.closest('form').requestSubmit()}" {}
                    }
                    form method="post" action=(format!("/reconcile/{}/reconciled/csv", session_id))
                          enctype="multipart/form-data" style="display:none" {
                        input type="file" name="csv_file" accept=".csv"
                               id=(format!("file-reconciled-{}", session_id))
                               onchange="if(this.files.length){this.closest('form').requestSubmit()}" {}
                    }
                }
                @if !unmatched_outgoing.is_empty() || !unmatched_reconciled.is_empty() {
                    form method="post" action=(format!("/reconcile/{}/auto-match", session_id)) class="auto-match-form" {
                        button type="submit" class="btn" { "Auto-Match" }
                    }
                }
            }

            // ── Sort toggle ──
            div class="reconcile-sort-toggle" {
                @let other_sort = match sort {
                    reconcile::SortOrder::Date => reconcile::SortOrder::Amount,
                    reconcile::SortOrder::Amount => reconcile::SortOrder::Date,
                };
                @let current_label = match sort {
                    reconcile::SortOrder::Date => "Sorted by date (newest first)",
                    reconcile::SortOrder::Amount => "Sorted by amount (highest first)",
                };
                span { (current_label) }
                a href=(format!("/reconcile/{}?sort={}", session_id, other_sort)) class="btn btn-sm" {
                    @match other_sort {
                        reconcile::SortOrder::Date => "Sort by date",
                        reconcile::SortOrder::Amount => "Sort by amount",
                    }
                }
            }

            // ── Summary: total unmatched amounts ──
            @let unmatched_outgoing_total: i64 = unmatched_outgoing.iter().map(|o| o.amount).sum();
            @let unmatched_reconciled_total: i64 = unmatched_reconciled.iter().map(|r| r.amount).sum();
            @let overs = unmatched_outgoing_total - unmatched_reconciled_total;
            div class="reconcile-summary" {
                span { "Unmatched Outgoing: " (utils::format_cents(unmatched_outgoing_total)) }
                span { "Unmatched Reconciled: " (utils::format_cents(unmatched_reconciled_total)) }
                @if overs != 0 {
                    span class="reconcile-overs" { "Overs: " (utils::format_cents(overs)) }
                }
            }

            (render_reconcile_sections(session_id, sort, &unmatched_outgoing, &unmatched_reconciled, unmatched_max, &matched_outgoing, &match_map, &reconciled, &ignored_outgoing, &ignored_reconciled, ignored_max, None))
        },
        Some(&user),
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn render_reconcile_sections(
    session_id: Uuid,
    sort: reconcile::SortOrder,
    unmatched_outgoing: &[&OutgoingTxn],
    unmatched_reconciled: &[&ReconciledTxn],
    unmatched_max: usize,
    matched_outgoing: &[&OutgoingTxn],
    match_map: &std::collections::HashMap<Uuid, Vec<Uuid>>,
    reconciled: &[ReconciledTxn],
    ignored_outgoing: &[OutgoingTxn],
    ignored_reconciled: &[ReconciledTxn],
    ignored_max: usize,
    error: Option<&str>,
) -> maud::Markup {
    maud::html! {
        div id="reconcile-sections" {
            @if let Some(msg) = error {
                div class="error" { (msg) }
            }
            // ════════════════════════════════════════════════════════════════
            // SECTION 1: Un-reconciled
            // ════════════════════════════════════════════════════════════════
            h2 id="unreconciled-section" { "Un-reconciled" }
            @if unmatched_outgoing.is_empty() && unmatched_reconciled.is_empty() {
                p class="reconcile-empty" { "All transactions have been reconciled or ignored." }
            } @else {
                div class="reconcile-grid" {
                    div class="reconcile-grid-header" { "Outgoing" }
                    div class="reconcile-grid-header" { "Reconciled" }

                    @for i in 0..unmatched_max {
                        @if let Some(o) = unmatched_outgoing.get(i) {
                            div class="reconcile-txn reconcile-txn--unmatched" id=(format!("unmatched-out-{}", i)) {
                                div class="txn-row" {
                                    span class="txn-date" { (utils::format_date(o.date)) }
                                    @if !o.vendor.is_empty() {
                                        span class="txn-vendor" { (o.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(o.amount)) }
                                    button type="submit" name="outgoing_id" value=(o.txn_id) form="reconcile-match-form" class="btn btn-sm" { "Match" }
                                    form method="post" action=(format!("/reconcile/{}/ignore-outgoing/{}", session_id, o.txn_id)) class="txn-ignore-form"
                                        hx-post=(format!("/reconcile/{}/ignore-outgoing/{}", session_id, o.txn_id))
                                        hx-target="#reconcile-sections"
                                        hx-swap="morphdom" {
                                        input type="hidden" name="sort" value=(sort.to_string()) {}
                                        button type="submit" class="btn-ignore" { "Ignore" }
                                    }
                                }
                                @if !o.metadata.is_empty() {
                                    details class="txn-metadata" {
                                        summary { "Metadata" }
                                        table {
                                            @for (key, val) in &o.metadata {
                                                tr {
                                                    td class="txn-metadata-key" { (key) }
                                                    td { (val) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } @else {
                            div class="reconcile-grid-spacer" id=(format!("unmatched-out-{}", i)) {}
                        }
                        @if let Some(r) = unmatched_reconciled.get(i) {
                            div class="reconcile-txn reconcile-txn--unmatched" id=(format!("unmatched-rec-{}", i)) {
                                div class="txn-row" {
                                    input type="checkbox" name="reconciled_ids" value=(r.txn_id) form="reconcile-match-form" class="txn-card-checkbox" {}
                                    span class="txn-date" { (utils::format_date(r.date)) }
                                    @if !r.vendor.is_empty() {
                                        span class="txn-vendor" { (r.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(r.amount)) }
                                    form method="post" action=(format!("/reconcile/{}/ignore-reconciled/{}", session_id, r.txn_id)) class="txn-ignore-form"
                                        hx-post=(format!("/reconcile/{}/ignore-reconciled/{}", session_id, r.txn_id))
                                        hx-target="#reconcile-sections"
                                        hx-swap="morphdom" {
                                        input type="hidden" name="sort" value=(sort.to_string()) {}
                                        button type="submit" class="btn-ignore" { "Ignore" }
                                    }
                                }
                                @if !r.metadata.is_empty() {
                                    details class="txn-metadata" {
                                        summary { "Metadata" }
                                        table {
                                            @for (key, val) in &r.metadata {
                                                tr {
                                                    td class="txn-metadata-key" { (key) }
                                                    td { (val) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } @else {
                            div class="reconcile-grid-spacer" id=(format!("unmatched-rec-{}", i)) {}
                        }
                    }
                }
            }

            // ════════════════════════════════════════════════════════════════
            // SECTION 2: Matched
            // ════════════════════════════════════════════════════════════════
            h2 id="matched-section" { "Matched" }
            @if matched_outgoing.is_empty() {
                p class="reconcile-empty" { "No matched transactions yet." }
            } @else {
                div class="reconcile-grid" {
                    div class="reconcile-grid-header" { "Outgoing" }
                    div class="reconcile-grid-header" { "Reconciled" }

                    @for o in matched_outgoing {
                        @if let Some(linked_ids) = match_map.get(&o.txn_id) {
                            @let row_span = linked_ids.len().max(1);
                            div class="reconcile-txn reconcile-txn--matched" style=(format!("grid-row: span {}", row_span)) {
                                div class="txn-row" {
                                    span class="txn-date" { (utils::format_date(o.date)) }
                                    @if !o.vendor.is_empty() {
                                        span class="txn-vendor" { (o.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(o.amount)) }
                                    @for rid in linked_ids {
                                        span class="txn-match-tag" {
                                            (utils::format_cents(reconciled.iter().find(|x| x.txn_id == *rid).map(|r| r.amount).unwrap_or(0)))
                                        }
                                    }
                                    @let reconciled_sum: i64 = linked_ids.iter()
                                        .filter_map(|rid| reconciled.iter().find(|x| x.txn_id == *rid).map(|r| r.amount))
                                        .sum();
                                    @let diff = reconciled_sum - o.amount;
                                    @if diff != 0 {
                                        span class="txn-diff" {
                                            @if diff > 0 {
                                                (format!("Over {}", utils::format_cents(diff)))
                                            } @else {
                                                (format!("Under {}", utils::format_cents(diff.abs())))
                                            }
                                        }
                                    }
                                    form method="post" action=(format!("/reconcile/{}/unlink", session_id)) class="txn-unlink-form"
                                        hx-post=(format!("/reconcile/{}/unlink", session_id))
                                        hx-target="#reconcile-sections"
                                        hx-swap="morphdom" {
                                        input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                        button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Unmatch" }
                                    }
                                }
                                @if !o.metadata.is_empty() {
                                    details class="txn-metadata" {
                                        summary { "Metadata" }
                                        table {
                                            @for (key, val) in &o.metadata {
                                                tr {
                                                    td class="txn-metadata-key" { (key) }
                                                    td { (val) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            @for rid in linked_ids {
                                @if let Some(r) = reconciled.iter().find(|x| x.txn_id == *rid) {
                                    div class="reconcile-txn reconcile-txn--matched" {
                                        div class="txn-row" {
                                            span class="txn-date" { (utils::format_date(r.date)) }
                                            @if !r.vendor.is_empty() {
                                                span class="txn-vendor" { (r.vendor) }
                                            }
                                            span class="txn-amount" { (utils::format_cents(r.amount)) }
                                            form method="post" action=(format!("/reconcile/{}/unlink-reconciled", session_id)) class="txn-unlink-form"
                                                hx-post=(format!("/reconcile/{}/unlink-reconciled", session_id))
                                                hx-target="#reconcile-sections"
                                                hx-swap="morphdom" {
                                                input type="hidden" name="reconciled_id" value=(r.txn_id) {}
                                                button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Unmatch" }
                                            }
                                        }
                                        @if !r.metadata.is_empty() {
                                            details class="txn-metadata" {
                                                summary { "Metadata" }
                                                table {
                                                    @for (key, val) in &r.metadata {
                                                        tr {
                                                            td class="txn-metadata-key" { (key) }
                                                            td { (val) }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ════════════════════════════════════════════════════════════════
            // SECTION 3: Ignored
            // ════════════════════════════════════════════════════════════════
            h2 id="ignored-section" { "Ignored" }
            @if ignored_outgoing.is_empty() && ignored_reconciled.is_empty() {
                p class="reconcile-empty" { "No ignored transactions." }
            } @else {
                div class="reconcile-grid" {
                    div class="reconcile-grid-header" { "Outgoing" }
                    div class="reconcile-grid-header" { "Reconciled" }

                    @for i in 0..ignored_max {
                        @if let Some(o) = ignored_outgoing.get(i) {
                            div class="reconcile-txn reconcile-txn--ignored" {
                                div class="txn-row" {
                                    span class="txn-date" { (utils::format_date(o.date)) }
                                    @if !o.vendor.is_empty() {
                                        span class="txn-vendor" { (o.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(o.amount)) }
                                    form method="post" action=(format!("/reconcile/{}/unignore-outgoing/{}", session_id, o.txn_id)) class="txn-ignore-form"
                                        hx-post=(format!("/reconcile/{}/unignore-outgoing/{}", session_id, o.txn_id))
                                        hx-target="#reconcile-sections"
                                        hx-swap="morphdom" {
                                        button type="submit" class="btn-undo" { "Undo" }
                                    }
                                }
                                @if !o.metadata.is_empty() {
                                    details class="txn-metadata" {
                                        summary { "Metadata" }
                                        table {
                                            @for (key, val) in &o.metadata {
                                                tr {
                                                    td class="txn-metadata-key" { (key) }
                                                    td { (val) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } @else {
                            div class="reconcile-grid-spacer" {}
                        }
                        @if let Some(r) = ignored_reconciled.get(i) {
                            div class="reconcile-txn reconcile-txn--ignored" {
                                div class="txn-row" {
                                    span class="txn-date" { (utils::format_date(r.date)) }
                                    @if !r.vendor.is_empty() {
                                        span class="txn-vendor" { (r.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(r.amount)) }
                                    form method="post" action=(format!("/reconcile/{}/unignore-reconciled/{}", session_id, r.txn_id)) class="txn-ignore-form"
                                        hx-post=(format!("/reconcile/{}/unignore-reconciled/{}", session_id, r.txn_id))
                                        hx-target="#reconcile-sections"
                                        hx-swap="morphdom" {
                                        button type="submit" class="btn-undo" { "Undo" }
                                    }
                                }
                                @if !r.metadata.is_empty() {
                                    details class="txn-metadata" {
                                        summary { "Metadata" }
                                        table {
                                            @for (key, val) in &r.metadata {
                                                tr {
                                                    td class="txn-metadata-key" { (key) }
                                                    td { (val) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } @else {
                            div class="reconcile-grid-spacer" {}
                        }
                    }
                }
            }
        }
    }
}
