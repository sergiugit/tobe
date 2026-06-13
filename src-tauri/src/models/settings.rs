use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub invidious_instance: String,
    pub use_scraper_fallback: bool,
    pub default_sort: String,
    pub theme: String,
    pub autoplay_next: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            invidious_instance: "https://inv.nadeko.net".to_string(),
            use_scraper_fallback: false,
            default_sort: "newest".to_string(),
            theme: "dark".to_string(),
            autoplay_next: true,
        }
    }
}
