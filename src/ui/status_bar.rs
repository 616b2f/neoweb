#[derive(Debug, Clone)]
pub struct StatusBarProps<'a> {
    pub mode: &'a str,
    pub uri: &'a str,
    pub hover_uri: Option<&'a str>,
    pub progress: f32,
    pub key_buffer: String,
    pub tab_index: usize,
    pub total_tabs: usize,
    pub scroll_pct: f32,
}
