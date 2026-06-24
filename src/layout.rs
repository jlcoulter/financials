use crate::cookies::LoggedInUser;
use crate::flash::Flash;

pub fn layout(title: &str, content: maud::Markup, username: Option<&LoggedInUser>, wide: bool) -> maud::Markup {
    render(title, content, username, wide, "", &Flash::default())
}

pub fn active(title: &str, content: maud::Markup, username: Option<&LoggedInUser>, wide: bool, page: &str) -> maud::Markup {
    render(title, content, username, wide, page, &Flash::default())
}

pub fn active_flash(title: &str, content: maud::Markup, username: Option<&LoggedInUser>, wide: bool, page: &str, flash: &Flash) -> maud::Markup {
    render(title, content, username, wide, page, flash)
}

fn render(title: &str, content: maud::Markup, username: Option<&LoggedInUser>, wide: bool, active: &str, flash: &Flash) -> maud::Markup {
    let body_class = if wide { "wide" } else { "" };
    let nav_items = [
        ("/dashboard", "Dashboard", "dashboard"),
        ("/portfolios", "Portfolios", "portfolios"),
        ("/stats", "Charts & Stats", "stats"),
        ("/transactions", "Transactions", "transactions"),
        ("/budgets", "Budgets", "budgets"),
        ("/goals", "Goals", "goals"),
        ("/holidays", "Holidays", "holidays"),
        ("/reconciliation", "Recon", "reconciliation"),
    ];

    maud::html! {
        html {
            head {
                title { (title) }
                meta name="viewport" content="width=device-width, initial-scale=1" {}
                script src="/static/htmx.min.js" {}
                script {
                    (maud::PreEscaped("document.addEventListener('htmx:beforeSwap', function(e) { if(e.detail.xhr.status >= 400) e.detail.shouldSwap = true; });"))
                }
                script {
                    (maud::PreEscaped("setTimeout(function(){ var f = document.getElementById('flash'); if(f) f.style.opacity = '0'; setTimeout(function(){ var f2 = document.getElementById('flash'); if(f2) f2.remove(); }, 400); }, 4000);"))
                }
                link rel="stylesheet" href="/static/style.css"{}
            }
            body class=(body_class) {
                nav {
                    a href="/" class="nav-brand" { "Financials" }
                    div class="nav-links" {
                        @for (href, label, id) in &nav_items {
                            @let cls = if *id == active { "nav-link active" } else { "nav-link" };
                            a href=(href) class=(cls) { (label) }
                        }
                    }
                    div class="nav-user" {
                        @if let Some(name) = username {
                            span class="nav-username" { (name.0) }
                            form action="/logout" method="post" style="display:inline" {
                                button type="submit" class="btn btn-ghost btn-sm" { "Logout" }
                            }
                        } @else {
                            a href="/login" class="btn btn-sm" { "Login" }
                            a href="/signup" class="btn btn-sm" { "Sign up" }
                        }
                    }
                }
                main class=(if wide { "main-wide" } else { "" }) {
                    (flash.render())
                    (content)
                }
            }
        }
    }
}

pub fn error_box(message: &str) -> maud::Markup {
    maud::html! {
        div class="error" { (message) }
    }
}