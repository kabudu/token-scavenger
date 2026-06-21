use crate::app::state::AppState;
use crate::config::schema::UpdateConfig;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub enabled: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub asset_name: Option<String>,
}

pub async fn check_for_update(config: &UpdateConfig) -> Result<UpdateStatus, UpdateError> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    if !config.enabled {
        return Ok(UpdateStatus {
            enabled: false,
            current_version: current,
            latest_version: None,
            update_available: false,
            release_url: None,
            asset_name: None,
        });
    }

    let release = fetch_latest_release(config).await?;
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let asset_name = platform_asset_name(&release.tag_name);
    let update_available = version_gt(&latest, &current);
    Ok(UpdateStatus {
        enabled: true,
        current_version: current,
        latest_version: Some(latest),
        update_available,
        release_url: Some(release.html_url),
        asset_name,
    })
}

pub async fn apply_update(state: AppState) -> Result<UpdateStatus, UpdateError> {
    let config = state.config().updates.clone();
    if !config.enabled {
        return Err(UpdateError::Disabled);
    }
    let release = fetch_latest_release(&config).await?;
    let current = env!("CARGO_PKG_VERSION");
    let latest = release.tag_name.trim_start_matches('v').to_string();
    if !version_gt(&latest, current) {
        return Ok(UpdateStatus {
            enabled: true,
            current_version: current.to_string(),
            latest_version: Some(latest),
            update_available: false,
            release_url: Some(release.html_url),
            asset_name: platform_asset_name(&release.tag_name),
        });
    }

    let asset_name =
        platform_asset_name(&release.tag_name).ok_or(UpdateError::UnsupportedPlatform)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| UpdateError::MissingAsset(asset_name.clone()))?;
    let checksums = release
        .assets
        .iter()
        .find(|asset| asset.name == "checksums.txt")
        .ok_or(UpdateError::MissingChecksums)?;

    let client = github_client()?;
    let binary_bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let checksum_text = client
        .get(&checksums.browser_download_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    verify_checksum(&asset.name, &binary_bytes, &checksum_text)?;

    let executable = extract_executable(&asset.name, &binary_bytes)?;
    let current_exe =
        std::env::current_exe().map_err(|error| UpdateError::Io(error.to_string()))?;
    let tmp = replacement_path(&current_exe);
    std::fs::write(&tmp, executable).map_err(|error| UpdateError::Io(error.to_string()))?;
    make_executable(&tmp)?;

    schedule_replace_and_restart(tmp, std::env::args_os().collect());

    Ok(UpdateStatus {
        enabled: true,
        current_version: current.to_string(),
        latest_version: Some(latest),
        update_available: true,
        release_url: Some(release.html_url),
        asset_name: Some(asset_name),
    })
}

fn schedule_replace_and_restart(new_binary: PathBuf, args: Vec<OsString>) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(750)).await;
        if let Ok(current) = std::env::current_exe() {
            let backup = current.with_extension("previous");
            let _ = std::fs::rename(&current, &backup);
            if std::fs::rename(&new_binary, &current).is_ok() {
                let mut command = std::process::Command::new(&current);
                command.args(args.into_iter().skip(1));
                let _ = command.spawn();
                std::process::exit(0);
            } else {
                let _ = std::fs::rename(&backup, &current);
            }
        }
    });
}

fn replacement_path(current: &Path) -> PathBuf {
    current.with_extension(format!("update-{}", chrono::Utc::now().timestamp_millis()))
}

fn extract_executable(asset_name: &str, bytes: &[u8]) -> Result<Vec<u8>, UpdateError> {
    if asset_name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| UpdateError::Archive(error.to_string()))?;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|error| UpdateError::Archive(error.to_string()))?;
            let name = file.name().to_string();
            if name.ends_with("tokenscavenger") || name.ends_with("tokenscavenger.exe") {
                let mut extracted = Vec::new();
                std::io::copy(&mut file, &mut extracted)
                    .map_err(|error| UpdateError::Io(error.to_string()))?;
                return Ok(extracted);
            }
        }
        Err(UpdateError::Archive(
            "archive did not contain tokenscavenger binary".into(),
        ))
    } else {
        Ok(bytes.to_vec())
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|error| UpdateError::Io(error.to_string()))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(|error| UpdateError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UpdateError> {
    Ok(())
}

fn verify_checksum(asset_name: &str, bytes: &[u8], checksums: &str) -> Result<(), UpdateError> {
    let expected = checksums
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?;
            (name == asset_name).then(|| hash.to_string())
        })
        .ok_or_else(|| UpdateError::MissingChecksum(asset_name.to_string()))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err(UpdateError::ChecksumMismatch)
    }
}

fn platform_asset_name(tag: &str) -> Option<String> {
    let version = tag.trim_start_matches('v');
    let suffix = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin.zip"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc.exe"
    } else {
        return None;
    };
    Some(format!("tokenscavenger-v{version}-{suffix}"))
}

fn version_gt(left: &str, right: &str) -> bool {
    let parse = |value: &str| {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(left) > parse(right)
}

async fn fetch_latest_release(config: &UpdateConfig) -> Result<GithubRelease, UpdateError> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        config.github_repo
    );
    Ok(github_client()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<GithubRelease>()
        .await?)
}

fn github_client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("tokenscavenger/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(UpdateError::Http)
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("self-update is disabled")]
    Disabled,
    #[error("unsupported platform for self-update")]
    UnsupportedPlatform,
    #[error("missing release asset {0}")]
    MissingAsset(String),
    #[error("release is missing checksums.txt")]
    MissingChecksums,
    #[error("checksums.txt has no entry for {0}")]
    MissingChecksum(String),
    #[error("downloaded artifact checksum did not match checksums.txt")]
    ChecksumMismatch,
    #[error("archive error: {0}")]
    Archive(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_like_versions() {
        assert!(version_gt("0.3.4", "0.3.3"));
        assert!(version_gt("0.4.0", "0.3.99"));
        assert!(!version_gt("0.3.3", "0.3.3"));
    }

    #[test]
    fn verifies_matching_checksum() {
        let bytes = b"hello";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let checksums = format!("{hash}  artifact");
        verify_checksum("artifact", bytes, &checksums).unwrap();
    }
}
