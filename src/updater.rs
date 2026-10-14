use serde::Deserialize;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_REPO: &str = "AgastyaShenoy/ao3reader";

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub current_version: String,
    pub release_name: String,
    pub release_notes: String,
    pub release_url: String,
    pub direct_download_url: Option<String>,
    pub published_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate { checked_version: String },
    UpdateAvailable(Box<UpdateInfo>),
    Error(String),
}

/// Parses a semver-like version string into (major, minor, patch).
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let clean = v.trim().trim_start_matches('v').trim_start_matches('V');
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    let major = parts.first()?.parse().ok()?;
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.split('-').next()?.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn is_newer_version(latest_tag: &str, current_ver: &str) -> bool {
    match (parse_version(latest_tag), parse_version(current_ver)) {
        (Some(latest), Some(curr)) => latest > curr,
        _ => false,
    }
}

pub async fn check_for_updates() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("AO3Reader-Desktop/1.0 (Windows; Rust; +https://github.com/AgastyaShenoy/ao3reader)")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let res = client.get(&url).send().await.map_err(|e| format!("Network request failed: {e}"))?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        // No releases published yet
        return Ok(None);
    }

    if !res.status().is_success() {
        return Err(format!("GitHub API returned HTTP status {}", res.status()));
    }

    let release = res.json::<GitHubRelease>().await.map_err(|e| format!("Failed to parse release metadata: {e}"))?;

    if release.draft {
        return Ok(None);
    }

    if is_newer_version(&release.tag_name, CURRENT_VERSION) {
        let direct_download_url = release
            .assets
            .iter()
            .find(|a| a.name.ends_with(".exe") || a.name.ends_with(".zip"))
            .map(|a| a.browser_download_url.clone());

        let published_date = release
            .published_at
            .as_deref()
            .and_then(|d| d.split('T').next())
            .unwrap_or("Recently")
            .to_string();

        Ok(Some(UpdateInfo {
            latest_version: release.tag_name.clone(),
            current_version: CURRENT_VERSION.to_string(),
            release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
            release_notes: release.body.unwrap_or_default(),
            release_url: release.html_url,
            direct_download_url,
            published_date,
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing_and_comparison() {
        assert_eq!(parse_version("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("V2.0.1-beta"), Some((2, 0, 1)));

        assert!(is_newer_version("v1.0.1", "1.0.0"));
        assert!(is_newer_version("v1.1.0", "1.0.9"));
        assert!(is_newer_version("v2.0.0", "1.9.9"));
        assert!(!is_newer_version("v1.0.0", "1.0.0"));
        assert!(!is_newer_version("v0.9.9", "1.0.0"));
    }
}
