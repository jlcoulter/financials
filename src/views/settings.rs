use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::{backup, portfolio, user};
use crate::requests::SettingsFlash;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use uuid::Uuid;

pub async fn dashboard(State(state): State<AppState>, user: LoggedInUser) -> impl IntoResponse {
    let username = user::get_username_by_id(&state.db().await, user.0)
        .await
        .unwrap_or_else(|_| "User".to_string());
    let hour = chrono::Local::now()
        .format("%H")
        .to_string()
        .parse::<u32>()
        .unwrap_or(12);
    let greeting = match hour {
        0..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    layout(
        "Dashboard",
        maud::html! {
            h2 { (greeting) ", " (username) }
            div class="cards" {
                a href="/portfolios" class="card" {
                    h3 { "Portfolios" }
                    p { "View and manage your portfolios" }
                }
                a href="/insights" class="card" {
                    h3 { "Insights" }
                    p { "View your financial insights" }
                }
                a href="/reconcile" class="card" {
                    h3 { "Reconcile" }
                    p { "Match outgoing transactions to reconciled records" }
                }
                a href="/settings" class="card" {
                    h3 { "Settings" }
                    p { "Configure backups and preferences" }
                }
            }
        },
        Some(&user),
    )
}

pub async fn insights(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(&state.db().await, user.0).await?;

    // Build portfolio selector links
    let portfolio_links: Vec<maud::Markup> = portfolios
        .iter()
        .map(|(pid, pname)| {
            maud::html! {
                a href=(format!("/insights/{}", pid)) class="insights-portfolio-link" { (pname) }
            }
        })
        .collect();

    Ok(layout(
        "Insights",
        maud::html! {
            h2 { "Insights" }
            div class="insights-portfolio-list" {
                @for link in &portfolio_links {
                    (link)
                }
            }
        },
        Some(&user),
    ))
}

pub async fn insights_chart(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(portfolio_id): Path<Uuid>,
) -> Result<maud::Markup, AppError> {
    use charming::datatype::DataPoint;
    use charming::element::smoothness::Smoothness;
    use charming::element::{AxisLabel, TextStyle};
    use charming::renderer::HtmlRenderer;
    use charming::series::Bar;
    use charming::series::Pie;
    use charming::{
        Chart,
        component::{Axis, Legend, Title},
        element::{AreaStyle, AxisType, Tooltip, Trigger},
        series::Line,
        theme::Theme,
    };

    let portfolios = portfolio::list_portfolios(&state.db().await, user.0).await?;
    let portfolio_name = portfolios
        .iter()
        .find(|(pid, _)| pid == &portfolio_id)
        .map(|(_, pname)| pname.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;

    // Get unique dates sorted ascending
    let mut dates: Vec<String> = logs
        .iter()
        .map(|l| l.log_date.format("%Y-%m-%d").to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    dates.sort();

    let mut item_names: Vec<String> = Vec::new();
    let mut values: Vec<Vec<f64>> = Vec::new();

    for item in &items {
        let item_logs: Vec<_> = logs.iter().filter(|l| l.item_id == item.item_id).collect();

        let mut row = vec![0.0; dates.len()];
        for log in &item_logs {
            let date_str = log.log_date.format("%Y-%m-%d").to_string();
            if let Some(idx) = dates.iter().position(|d| d == &date_str) {
                let val = log.balance_value as f64 / 100.0;
                row[idx] = val;
            }
        }

        item_names.push(item.name.clone());
        values.push(row);
    }

    // Build portfolio selector links
    let portfolio_links: Vec<maud::Markup> = portfolios.iter().map(|(pid, pname)| {
        let current = *pid == portfolio_id;
        maud::html! {
            a href=(format!("/insights/{}", pid))
               class=(if current { "insights-portfolio-link current" } else { "insights-portfolio-link" }) { (pname) }
        }
    }).collect();

    // Chart A: Cumulative Net Worth Trend (stacked area line)
    let white_text = TextStyle::new().color("#ffffff");
    let white_axis_label = AxisLabel::new().color("#ffffff");

    let mut trend_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Net Worth Trend", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Axis))
        .legend(
            Legend::new()
                .data(item_names.clone())
                .text_style(white_text.clone())
                .top("30"),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(dates.clone())
                .axis_label(white_axis_label.clone()),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .axis_label(white_axis_label.clone()),
        );

    for (i, name) in item_names.iter().enumerate() {
        let series = Line::new()
            .name(name.clone())
            .stack("total")
            .area_style(AreaStyle::new().opacity(0.3))
            .smooth(Smoothness::Boolean(true))
            .data(values[i].clone());
        trend_chart = trend_chart.series(series);
    }

    let trend_html = HtmlRenderer::new("trend-chart", 900, 500)
        .theme(Theme::Dark)
        .render(&trend_chart)
        .unwrap_or_else(|_| "<p>Trend chart rendering failed</p>".to_string());

    // Chart B: Cash Flow (grouped bar — positive = income, negative = expenses)
    // Compute per-date totals for inflows vs outflows
    let mut inflow: Vec<f64> = vec![0.0; dates.len()];
    let mut outflow: Vec<f64> = vec![0.0; dates.len()];
    for (i, name) in item_names.iter().enumerate() {
        let item = items.iter().find(|it| &it.name == name).unwrap();
        for (j, &val) in values[i].iter().enumerate() {
            if item.item_type == "debt" {
                outflow[j] += val.abs();
            } else {
                inflow[j] += val;
            }
        }
    }

    let mut flow_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Cash Flow", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Axis))
        .legend(
            Legend::new()
                .data(vec!["Income".to_string(), "Expenses".to_string()])
                .text_style(white_text.clone())
                .top("30"),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(dates.clone())
                .axis_label(white_axis_label.clone()),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .axis_label(white_axis_label.clone()),
        );

    flow_chart = flow_chart
        .series(Bar::new().name("Income").data(inflow))
        .series(Bar::new().name("Expenses").data(outflow));

    let flow_html = HtmlRenderer::new("flow-chart", 900, 400)
        .theme(Theme::Dark)
        .render(&flow_chart)
        .unwrap_or_else(|_| "<p>Flow chart rendering failed</p>".to_string());

    // Chart C: Asset Allocation (donut pie)
    // Compute latest values per item (use last non-zero, or last date's value)
    let mut pie_data: Vec<(String, f64)> = Vec::new();
    for (i, name) in item_names.iter().enumerate() {
        let latest = values[i]
            .iter()
            .rev()
            .find(|&&v| v != 0.0)
            .copied()
            .unwrap_or(0.0);
        if latest > 0.0 {
            pie_data.push((name.clone(), latest));
        }
    }

    let pie_series_data: Vec<DataPoint> = pie_data
        .iter()
        .map(|(name, val)| {
            DataPoint::Item(charming::datatype::DataPointItem::new(*val).name(name.clone()))
        })
        .collect();

    let pie_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Asset Allocation", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Item))
        .legend(
            Legend::new()
                .data(pie_data.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>())
                .text_style(white_text.clone())
                .bottom("0%")
                .left("center"),
        )
        .series(
            Pie::new()
                .name("Allocation")
                .radius(vec!["40%", "70%"])
                .data(pie_series_data),
        );

    let pie_html = HtmlRenderer::new("pie-chart", 900, 500)
        .theme(Theme::Dark)
        .render(&pie_chart)
        .unwrap_or_else(|_| "<p>Pie chart rendering failed</p>".to_string());

    // Replace hardcoded "chart" ids in charming HTML with unique ids
    // (charming hardcodes id="chart" for every render)
    fn make_chart_id(html: &str, new_id: &str) -> String {
        html.replace("id=\"chart\"", &format!("id=\"{}\"", new_id))
            .replace(
                "getElementById('chart')",
                &format!("getElementById('{}')", new_id),
            )
    }

    let trend_html = make_chart_id(&trend_html, "trend-chart");
    let flow_html = make_chart_id(&flow_html, "flow-chart");
    let pie_html = make_chart_id(&pie_html, "pie-chart");

    Ok(layout(
        "Insights",
        maud::html! {
            h2 { "Insights" }
            div class="insights-portfolio-list" {
                @for link in &portfolio_links {
                    (link)
                }
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(trend_html))
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(flow_html))
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(pie_html))
            }
        },
        Some(&user),
    ))
}

pub async fn settings(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(params): Query<SettingsFlash>,
) -> Result<maud::Markup, AppError> {
    let config = backup::get_config(&state.db().await).await?;
    let username = user::get_username_by_id(&state.db().await, user.0)
        .await
        .unwrap_or_else(|_| "User".to_string());

    let flash = params.flash.as_deref();

    let (provider, bucket, path, region, endpoint, access_key_id, b2_key_id, b2_endpoint, _enabled) =
        match &config {
            Some(c) => (
                c.provider.clone(),
                c.bucket.clone(),
                c.path.clone(),
                c.region.clone(),
                c.endpoint.clone(),
                c.access_key_id.clone(),
                c.b2_key_id.clone(),
                c.b2_endpoint.clone(),
                c.enabled,
            ),
            None => (
                "s3".to_string(),
                String::new(),
                "financials-backups".to_string(),
                "us-east-1".to_string(),
                None,
                None,
                None,
                None,
                false,
            ),
        };

    let s3_style = if provider == "s3" { "" } else { "display:none" };
    let b2_style = if provider == "b2" { "" } else { "display:none" };

    let provider_options = if provider == "s3" {
        maud::html! {
            option value="s3" selected { "Amazon S3 / S3-compatible" }
            option value="b2" { "Backblaze B2" }
        }
    } else {
        maud::html! {
            option value="s3" { "Amazon S3 / S3-compatible" }
            option value="b2" selected { "Backblaze B2" }
        }
    };

    let enable_disable_btn = match &config {
        Some(c) if c.enabled => Some(maud::html! {
            button type="submit" formaction="/settings/backup/disable" class="btn btn-ghost" { "Pause Backups" }
        }),
        Some(_) => Some(maud::html! {
            button type="submit" formaction="/settings/backup/enable" class="btn" { "Enable Backups" }
        }),
        None => None,
    };

    // Restore points are loaded asynchronously via HTMX
    // to avoid blocking the settings page on listing remote snapshots

    Ok(layout(
        "Settings",
        maud::html! {
            h2 { "Settings" }
            p { "Hello, " (username) }

            div class="settings-tabs" {
                button class="tab-btn active" data-tab="backup" { "Backup" }
                button class="tab-btn" data-tab="restore" { "Restore" }
                button class="tab-btn" data-tab="password" { "Password" }
            }

            @if let Some(msg) = flash {
                @if msg == "saved" {
                    div class="flash flash-success" { "Configuration saved" }
                } @else if msg == "enabled" {
                    div class="flash flash-success" { "Backups enabled" }
                } @else if msg == "disabled" {
                    div class="flash flash-info" { "Backups paused" }
                } @else if msg == "restored" {
                    div class="flash flash-success" { "Database restored from backup" }
                } @else if msg == "restore_failed" {
                    div class="flash flash-error" { "Restore failed — check server logs for details" }
                } @else if msg == "password_changed" {
                    div class="flash flash-success" { "Password updated successfully" }
                }
            }

            div id="backup" class="tab-content" {
                h3 { "Database Backups" }
                p { "Automatically back up your financial data to cloud storage. Choose a provider and enter your credentials." }

                @if config.is_some() {
                    div class="backup-status" {
                        @if let Some(c) = &config {
                            @if c.enabled {
                                div class="flash flash-success" { "Backups are active" }
                            } @else {
                                div class="flash flash-warning" { "Backups are paused" }
                            }
                        }
                        p class="backup-detail" {
                            "Provider: " (match &config { Some(c) => c.provider.clone(), None => String::new() })
                            " | Bucket: " (match &config { Some(c) => c.bucket.clone(), None => String::new() })
                        }
                    }
                }

                form action="/settings/backup" method="post" class="settings-form" {
                    label { "Provider"
                        select name="provider" id="provider-select" {
                            (provider_options)
                        }
                    }

                    label { "Bucket Name"
                        input type="text" name="bucket" value=(bucket) placeholder="my-backup-bucket";
                    }
                    label { "Backup Path Prefix"
                        input type="text" name="path" value=(path) placeholder="financials-backups";
                    }
                    label { "Snapshot interval (minutes)"
                        input type="number" name="interval_minutes" value=(config.as_ref().map(|c| c.interval_minutes.to_string()).unwrap_or_else(|| "60".to_string())) min="5" max="10080" placeholder="60";
                    }
                    label { "Max snapshots to keep"
                        input type="number" name="max_snapshots" value=(config.as_ref().map(|c| c.max_snapshots.to_string()).unwrap_or_else(|| "10".to_string())) min="1" max="1000" placeholder="10";
                    }

                    div id="s3-fields" style=(s3_style) {
                        label { "Region"
                            input type="text" name="region" value=(region) placeholder="us-east-1";
                        }
                        label { "Custom Endpoint (optional — leave empty for AWS)"
                            input type="text" name="endpoint" value=(endpoint.unwrap_or_default()) placeholder="https://s3.example.com";
                        }
                        label { "Access Key ID"
                            input type="text" name="access_key_id" value=(access_key_id.unwrap_or_default()) autocomplete="off";
                        }
                        label { "Secret Access Key"
                            input type="password" name="secret_access_key" autocomplete="new-password" placeholder="Enter your secret key";
                        }
                    }

                    div id="b2-fields" style=(b2_style) {
                        label { "S3 Endpoint"
                            input type="text" name="b2_endpoint" value=(b2_endpoint.unwrap_or_else(|| "s3.us-west-004.backblazeb2.com".to_string())) placeholder="s3.us-west-004.backblazeb2.com";
                        }
                        label { "Key ID"
                            input type="text" name="b2_key_id" value=(b2_key_id.unwrap_or_default()) autocomplete="off";
                        }
                        label { "Application Key"
                            input type="password" name="b2_application_key" autocomplete="new-password" placeholder="Enter your application key";
                        }
                    }

                    div class="settings-actions" {
                        button type="submit" class="btn" { "Save Configuration" }
                        @if let Some(btn) = enable_disable_btn {
                            (btn)
                        }
                    }
                }

                @if config.as_ref().is_some_and(|c| c.enabled) {
                    form action="/settings/backup/snapshot" method="post" class="settings-form" {
                        button type="submit" class="btn" { "Create Snapshot Now" }
                    }
                }
            }

            div id="restore" class="tab-content" style="display:none" {
                h3 { "Restore from Backup" }
                p { "Restore your database from a remote snapshot. \
                     This will replace your current database with the backup version." }

                @if config.is_some() {
                    div class="backup-status" {
                        p class="backup-detail" {
                            "Will restore from: "
                            (match &config { Some(c) => c.provider.clone(), None => String::new() })
                            " | Bucket: "
                            (match &config { Some(c) => c.bucket.clone(), None => String::new() })
                        }
                    }

                    div class="flash flash-warning" {
                        "Warning: This will replace your current database with the backup. \
                         Any changes made since the backup point will be lost."
                    }

                    form action="/settings/backup/restore" method="post" class="settings-form" {
                        @if config.is_some() {
                            div class="form-group" {
                                label { "Restore point"
                                    div id="restore-points" hx-get="/settings/backup/restore-points" hx-trigger="load" hx-swap="innerHTML" {
                                        select name="timestamp" disabled {
                                            option { "Loading..." }
                                        }
                                    }
                                }
                                p class="form-hint" { "Each entry is a full snapshot of the database at that point in time. \
                                    Choose the snapshot you want to restore to, or select Latest for the most recent." }
                            }
                        } @else {
                            p class="form-hint" { "No backup configuration found. Configure backups first." }
                        }
                        div class="settings-actions" {
                            button type="submit" class="btn btn-ghost" { "Restore from Backup" }
                        }
                    }
                } @else {
                    div class="flash flash-warning" {
                        "No backup configuration found. Configure backups first."
                    }
                }
            }

            div id="password" class="tab-content" style="display:none" {
                h3 { "Password" }
                p { "Change your admin login password." }
                a href="/change-password" class="btn" { "Change Password" }
            }

            script type="text/javascript" {
                (maud::PreEscaped("document.getElementById('provider-select').addEventListener('change', function() { var showS3 = this.value === 's3'; var showB2 = this.value === 'b2'; document.getElementById('s3-fields').style.display = showS3 ? '' : 'none'; document.getElementById('b2-fields').style.display = showB2 ? '' : 'none'; document.querySelectorAll('#s3-fields input, #s3-fields select').forEach(function(el) { el.disabled = !showS3; }); document.querySelectorAll('#b2-fields input, #b2-fields select').forEach(function(el) { el.disabled = !showB2; }); }); document.querySelectorAll('.tab-btn').forEach(function(btn) { btn.addEventListener('click', function() { document.querySelectorAll('.tab-btn').forEach(function(b) { b.classList.remove('active'); }); document.querySelectorAll('.tab-content').forEach(function(t) { t.style.display = 'none'; }); btn.classList.add('active'); document.getElementById(btn.dataset.tab).style.display = ''; }); }); (function() { var p = document.getElementById('provider-select').value; document.querySelectorAll('#s3-fields input, #s3-fields select').forEach(function(el) { el.disabled = p !== 's3'; }); document.querySelectorAll('#b2-fields input, #b2-fields select').forEach(function(el) { el.disabled = p !== 'b2'; }); })();"))
            }
        },
        Some(&user),
    ))
}

pub async fn backup_page(
    State(state): State<AppState>,
    Query(params): Query<SettingsFlash>,
) -> Result<maud::Markup, AppError> {
    // Try to load config from DB — may fail if DB is corrupt/missing
    let config = backup::get_config(&state.db().await).await.ok().flatten();

    let flash = params.flash.as_deref();

    let (provider, bucket, path, region, endpoint, access_key_id, b2_key_id, b2_endpoint) =
        match &config {
            Some(c) => (
                c.provider.clone(),
                c.bucket.clone(),
                c.path.clone(),
                c.region.clone(),
                c.endpoint.clone(),
                c.access_key_id.clone(),
                c.b2_key_id.clone(),
                c.b2_endpoint.clone(),
            ),
            None => (
                "s3".to_string(),
                String::new(),
                "financials-backups".to_string(),
                "us-east-1".to_string(),
                None,
                None,
                None,
                None,
            ),
        };

    let s3_style = if provider == "s3" { "" } else { "display:none" };
    let b2_style = if provider == "b2" { "" } else { "display:none" };

    let provider_options = if provider == "s3" {
        maud::html! {
            option value="s3" selected { "Amazon S3 / S3-compatible" }
            option value="b2" { "Backblaze B2" }
        }
    } else {
        maud::html! {
            option value="s3" { "Amazon S3 / S3-compatible" }
            option value="b2" selected { "Backblaze B2" }
        }
    };

    let enable_disable_btn = match &config {
        Some(c) if c.enabled => Some(maud::html! {
            button type="submit" formaction="/backup/disable" class="btn btn-ghost" { "Pause Backups" }
        }),
        Some(_) => Some(maud::html! {
            button type="submit" formaction="/backup/enable" class="btn" { "Enable Backups" }
        }),
        None => None,
    };

    Ok(layout(
        "Backup & Restore",
        maud::html! {
            h2 { "Backup & Restore" }
            p { "Disaster recovery page — accessible without login." }

            div class="settings-tabs" {
                button class="tab-btn active" data-tab="backup" { "Backup" }
                button class="tab-btn" data-tab="restore" { "Restore" }
                button class="tab-btn" data-tab="password" { "Password" }
            }

            @if let Some(msg) = flash {
                @if msg == "saved" {
                    div class="flash flash-success" { "Configuration saved" }
                } @else if msg == "enabled" {
                    div class="flash flash-success" { "Backups enabled" }
                } @else if msg == "disabled" {
                    div class="flash flash-info" { "Backups paused" }
                } @else if msg == "restored" {
                    div class="flash flash-success" { "Database restored from backup" }
                } @else if msg == "restore_failed" {
                    div class="flash flash-error" { "Restore failed — check server logs for details" }
                } @else if msg == "password_changed" {
                    div class="flash flash-success" { "Password updated successfully" }
                }
            }

            div id="backup" class="tab-content" {
                h3 { "Database Backups" }
                p { "Automatically back up your financial data to cloud storage. Choose a provider and enter your credentials." }

                @if config.is_some() {
                    div class="backup-status" {
                        @if let Some(c) = &config {
                            @if c.enabled {
                                div class="flash flash-success" { "Backups are active" }
                            } @else {
                                div class="flash flash-warning" { "Backups are paused" }
                            }
                        }
                        p class="backup-detail" {
                            "Provider: " (match &config { Some(c) => c.provider.clone(), None => String::new() })
                            " | Bucket: " (match &config { Some(c) => c.bucket.clone(), None => String::new() })
                        }
                    }
                }

                form action="/backup/configure" method="post" class="settings-form" {
                    label { "Provider"
                        select name="provider" id="provider-select" {
                            (provider_options)
                        }
                    }

                    label { "Bucket"
                        input type="text" name="bucket" value=(bucket);
                    }
                    label { "Path (prefix)"
                        input type="text" name="path" value=(path) placeholder="db-backups";
                    }
                    label { "Snapshot interval (minutes)"
                        input type="number" name="interval_minutes" value=(config.as_ref().map(|c| c.interval_minutes.to_string()).unwrap_or_else(|| "60".to_string())) min="5" max="10080" placeholder="60";
                    }
                    label { "Max snapshots to keep"
                        input type="number" name="max_snapshots" value=(config.as_ref().map(|c| c.max_snapshots.to_string()).unwrap_or_else(|| "10".to_string())) min="1" max="1000" placeholder="10";
                    }

                    div id="s3-fields" style=(s3_style) {
                        label { "Region"
                            input type="text" name="region" value=(region);
                        }
                        label { "Endpoint (optional, for S3-compatible storage)"
                            input type="text" name="endpoint" value=(endpoint.unwrap_or_default());
                        }
                        label { "Access Key ID"
                            input type="text" name="access_key_id" value=(access_key_id.unwrap_or_default());
                        }
                        label { "Secret Access Key"
                            input type="password" name="secret_access_key" placeholder="(unchanged if blank)";
                        }
                    }

                    div id="b2-fields" style=(b2_style) {
                        label { "Region"
                            input type="text" name="b2_region" value=(region);
                        }
                        label { "B2 Key ID"
                            input type="text" name="b2_key_id" value=(b2_key_id.unwrap_or_default());
                        }
                        label { "B2 Application Key"
                            input type="password" name="b2_application_key" placeholder="(unchanged if blank)";
                        }
                        label { "B2 Endpoint (leave default unless custom)"
                            input type="text" name="b2_endpoint" value=(b2_endpoint.unwrap_or_default()) placeholder="s3.us-west-004.backblazeb2.com";
                        }
                    }

                    div class="form-actions" {
                        button type="submit" class="btn" { "Save Configuration" }
                        @if let Some(btn) = enable_disable_btn {
                            (btn)
                        }
                    }
                }

                @if config.as_ref().is_some_and(|c| c.enabled) {
                    form action="/backup/snapshot" method="post" class="settings-form" {
                        button type="submit" class="btn" { "Create Snapshot Now" }
                    }
                }
            }

            div id="restore" class="tab-content" style="display:none" {
                h3 { "Restore from Backup" }
                p { "Restore the database from a remote backup snapshot. This will replace the current database." }

                form action="/backup/restore" method="post" class="settings-form" {
                    label { "Restore Point"
                        div hx-get="/backup/restore-points" hx-trigger="load" hx-swap="innerHTML" {
                            "Loading restore points..."
                        }
                    }
                    button type="submit" class="btn btn-danger" { "Restore Database" }
                }
            }

            script {
                (maud::PreEscaped(
                    "document.querySelectorAll('.tab-btn').forEach(btn => {
                        btn.addEventListener('click', function() {
                            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
                            this.classList.add('active');
                            document.querySelectorAll('.tab-content').forEach(tc => tc.style.display = 'none');
                            document.getElementById(this.dataset.tab).style.display = 'block';
                        });
                    });
                    document.getElementById('provider-select').addEventListener('change', function() {
                        var showS3 = this.value === 's3';
                        var showB2 = this.value === 'b2';
                        document.getElementById('s3-fields').style.display = showS3 ? 'block' : 'none';
                        document.getElementById('b2-fields').style.display = showB2 ? 'block' : 'none';
                        document.querySelectorAll('#s3-fields input, #s3-fields select').forEach(function(el) { el.disabled = !showS3; });
                        document.querySelectorAll('#b2-fields input, #b2-fields select').forEach(function(el) { el.disabled = !showB2; });
                    });
                    // Init: disable inputs in the hidden provider section
                    (function() {
                        var p = document.getElementById('provider-select').value;
                        document.querySelectorAll('#s3-fields input, #s3-fields select').forEach(function(el) { el.disabled = p !== 's3'; });
                        document.querySelectorAll('#b2-fields input, #b2-fields select').forEach(function(el) { el.disabled = p !== 'b2'; });
                    })();"
                ))
            }
        },
        None,
    ))
}
