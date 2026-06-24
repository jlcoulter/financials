use serde::Deserialize;

#[derive(Deserialize, Debug, Default)]
pub struct Flash {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

impl Flash {
    /// Render a flash banner HTML if present
    pub fn render(&self) -> maud::Markup {
        match &self.flash {
            Some(msg) => {
                let cls = match self.flash_type.as_deref() {
                    Some("error") => "flash-error",
                    Some("warning") => "flash-warning",
                    _ => "flash-success",
                };
                maud::html! {
                    div class=(format!("flash {}", cls)) id="flash" {
                        span { (msg) }
                        button type="button" class="flash-close" onclick="document.getElementById('flash').remove()" { "×" }
                    }
                }
            }
            None => maud::html! {},
        }
    }
}