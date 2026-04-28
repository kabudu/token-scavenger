/// UI asset embedding (stub for MVP).
/// In release builds, compiled frontend assets would be embedded here.
/// For now, the UI is server-rendered HTML via the `ui::routes` module.
pub fn init_assets() {
    // Assets are served via the inline HTMX/Tailwind approach.
    // Future: use rust-embed or include_dir! for compiled CSS/JS.
}
