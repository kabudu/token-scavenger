pub mod config_cmd;
pub mod setup;

use std::path::PathBuf;

/// Standard config file paths checked in order.
pub fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Current directory
    paths.push(PathBuf::from("tokenscavenger.toml"));

    // ~/.config/tokenscavenger/tokenscavenger.toml
    if let Some(home) = dirs_next_path() {
        paths.push(
            home.join(".config")
                .join("tokenscavenger")
                .join("tokenscavenger.toml"),
        );
        paths.push(home.join(".tokenscavenger.toml"));
    }

    paths
}

/// Find the first config file that exists on disk.
pub fn find_existing_config() -> Option<PathBuf> {
    config_search_paths().into_iter().find(|p| p.exists())
}

/// Determine the default path for a new config file.
pub fn default_config_path() -> PathBuf {
    if let Some(home) = dirs_next_path() {
        home.join(".config")
            .join("tokenscavenger")
            .join("tokenscavenger.toml")
    } else {
        PathBuf::from("tokenscavenger.toml")
    }
}

fn dirs_next_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}
