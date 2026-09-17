use crate::cookies::LoggedInUser;

pub fn layout(
    title: &str,
    content: maud::Markup,
    logged_in: Option<&LoggedInUser>,
) -> maud::Markup {
    maud::html! {
        html {
            head {
                title { (title) }
                script src="/static/htmx.min.js" {}
                script {
                    (maud::PreEscaped("document.addEventListener('htmx:beforeSwap', function(e) { if(e.detail.xhr.status >= 400) e.detail.shouldSwap = true; }); document.addEventListener('htmx:confirm', function(e) { var f = e.detail.elt; if (!f || f.id !== 'reconcile-match-form') return; var trig = e.detail.triggeringEvent; var btn = trig && (trig.submitter || trig.target); var out = btn ? parseInt(btn.dataset.amount, 10) : 0; var checked = document.querySelectorAll('input[name=\"reconciled_ids\"]:checked'); if (checked.length === 0) return; var sum = 0; checked.forEach(function(c){ sum += parseInt(c.dataset.amount, 10); }); if (sum === out) return; var fmt = function(n) { return (n < 0 ? '-$' : '$') + (Math.abs(n)/100).toLocaleString(undefined, {minimumFractionDigits: 2, maximumFractionDigits: 2}); }; var diff = sum - out; var word = diff > 0 ? 'Over' : 'Under'; if (!confirm('Selected reconciled ' + fmt(sum) + ' vs outgoing ' + fmt(out) + ' - ' + word + ' ' + fmt(Math.abs(diff)) + '. Match anyway?')) { e.preventDefault(); } });"))
                }
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                header {
                    @if logged_in.is_some() {
                        a href="/dashboard" class="header-link" { "Dashboard" }
                        span { "Hello!" }
                        form action="/logout" method="post" {
                            button type="submit" class="btn btn-ghost" { "Logout" }
                        }
                    } @else {
                        a href="/login" class="btn" { "Login" }
                        a href="/backup" class="btn btn-ghost" { "Restore" }
                    }
                }
                (content)
            }
        }
    }
}

pub fn error_box(message: &str) -> maud::Markup {
    maud::html! {
        div class="error" { (message) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_html() -> String {
        layout("Test", maud::html! {}, None).into_string()
    }

    #[test]
    fn layout_registers_match_diff_confirm_listener() {
        let html = layout_html();
        assert!(
            html.contains("htmx:confirm"),
            "short-match confirm should hook htmx:confirm"
        );
        assert!(
            html.contains("reconciled_ids"),
            "listener should read reconciled checkbox amounts"
        );
    }

    #[test]
    fn listener_reads_outgoing_amount_from_submitter_button() {
        let html = layout_html();
        assert!(
            html.contains("trig.submitter"),
            "on form submit the outgoing amount must come from the clicked submitter button, not the form target"
        );
    }
}
