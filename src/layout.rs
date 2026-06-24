use crate::cookies::LoggedInUser;

pub fn layout(title: &str, content: maud::Markup, username: Option<&LoggedInUser>) -> maud::Markup {
    maud::html! {
        html {
            head {
                title { (title) }
                script src="/static/htmx.min.js" {}
                script {
                    (maud::PreEscaped("document.addEventListener('htmx:beforeSwap', function(e) { if(e.detail.xhr.status >= 400) e.detail.shouldSwap = true; });"))
                }
                link rel="stylesheet" href="/static/style.css"{}
            }
            body {
                header {
                    @if let Some(_name) = username {
                        span { "Hello!"  }
                        form action = "/logout"
                        method = "post" {
                            button type = "submit" class="btn btn-ghost" {"Logout"}
                        }
                    } @else {
                        a href = "/login" class="btn" { "Login" }
                        " "
                        a href = "/signup" class="btn" {"Sign up"}
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
