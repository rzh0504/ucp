use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/rzh0504/ucp/releases/latest";
const LATEST_RELEASE_URL: &str = "https://github.com/rzh0504/ucp/releases/latest";
const RELEASE_ACCEPT: &str = "application/vnd.github+json";
const RELEASE_TAG_PATH: &str = "/releases/tag/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_USER_AGENT: &str = concat!("ucp/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub asset_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate { latest_version: String },
    Available(UpdateInfo),
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

pub fn check_for_updates() -> Result<UpdateCheck, String> {
    let client = update_client()?;
    let release = fetch_latest_release(&client).or_else(|api_error| {
        fetch_latest_release_from_redirect(&client).map_err(|fallback_error| {
            format!("{api_error}; fallback release check failed: {fallback_error}")
        })
    })?;
    let current_version = parse_release_version(APP_VERSION)?;
    let latest_version = parse_release_version(&release.tag_name)?;

    if latest_version <= current_version {
        return Ok(UpdateCheck::UpToDate {
            latest_version: latest_version.to_string(),
        });
    }

    let release_url = release.html_url;
    let (download_url, asset_name) = preferred_asset(&release.assets)
        .map(|asset| (asset.browser_download_url.clone(), Some(asset.name.clone())))
        .unwrap_or_else(|| (release_url.clone(), None));

    Ok(UpdateCheck::Available(UpdateInfo {
        version: latest_version.to_string(),
        download_url,
        asset_name,
    }))
}

fn update_client() -> Result<Client, String> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("failed to create update client: {error}"))
}

fn fetch_latest_release(client: &Client) -> Result<GithubRelease, String> {
    client
        .get(LATEST_RELEASE_API)
        .header(USER_AGENT, REQUEST_USER_AGENT)
        .header(ACCEPT, RELEASE_ACCEPT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to fetch latest release: {error}"))?
        .json::<GithubRelease>()
        .map_err(|error| format!("failed to read latest release: {error}"))
}

fn fetch_latest_release_from_redirect(client: &Client) -> Result<GithubRelease, String> {
    let response = client
        .get(LATEST_RELEASE_URL)
        .header(USER_AGENT, REQUEST_USER_AGENT)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to fetch latest release page: {error}"))?;
    let release_url = response.url().to_string();
    let tag_name = release_tag_from_url(&release_url)
        .ok_or_else(|| format!("latest release page did not resolve to a tag: {release_url}"))?;

    Ok(GithubRelease {
        tag_name,
        html_url: release_url,
        assets: Vec::new(),
    })
}

fn release_tag_from_url(url: &str) -> Option<String> {
    let (_, tag) = url.split_once(RELEASE_TAG_PATH)?;
    let tag = tag.split(['?', '#', '/']).next().unwrap_or_default().trim();

    (!tag.is_empty()).then(|| tag.to_string())
}

fn parse_release_version(version: &str) -> Result<Version, String> {
    let version = version.trim();
    let version = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);

    Version::parse(version).map_err(|error| format!("invalid release version {version}: {error}"))
}

fn preferred_asset(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    assets
        .iter()
        .filter_map(|asset| asset_score(&asset.name).map(|score| (score, asset)))
        .min_by_key(|(score, _)| *score)
        .map(|(_, asset)| asset)
}

fn asset_score(name: &str) -> Option<u8> {
    let name = name.to_ascii_lowercase();

    if cfg!(target_os = "windows") {
        if name.ends_with(".exe") {
            Some(0)
        } else if name.ends_with(".msi") {
            Some(1)
        } else if name.ends_with(".zip") {
            Some(2)
        } else {
            None
        }
    } else if cfg!(target_os = "macos") {
        if name.ends_with(".dmg") {
            Some(0)
        } else if name.ends_with(".pkg") {
            Some(1)
        } else if name.ends_with(".zip") {
            Some(2)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(3)
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if name.ends_with(".appimage") {
            Some(0)
        } else if name.ends_with(".deb") {
            Some(1)
        } else if name.ends_with(".rpm") {
            Some(2)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(3)
        } else {
            None
        }
    } else {
        Some(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v_prefixed_versions() {
        assert_eq!(
            parse_release_version("v1.2.3").unwrap(),
            Version::new(1, 2, 3)
        );
        assert_eq!(
            parse_release_version("0.4.5").unwrap(),
            Version::new(0, 4, 5)
        );
    }

    #[test]
    fn extracts_release_tag_from_latest_redirect_url() {
        assert_eq!(
            release_tag_from_url("https://github.com/rzh0504/ucp/releases/tag/v0.2.0"),
            Some("v0.2.0".to_string())
        );
        assert_eq!(
            release_tag_from_url("https://github.com/rzh0504/ucp/releases/tag/v0.2.0?from=latest"),
            Some("v0.2.0".to_string())
        );
        assert_eq!(
            release_tag_from_url("https://github.com/rzh0504/ucp/releases"),
            None
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn prefers_windows_installer_asset() {
        let assets = vec![
            GithubAsset {
                name: "ucp-portable.zip".to_string(),
                browser_download_url: "https://example.com/ucp.zip".to_string(),
            },
            GithubAsset {
                name: "ucp-setup.exe".to_string(),
                browser_download_url: "https://example.com/ucp.exe".to_string(),
            },
        ];

        assert_eq!(preferred_asset(&assets).unwrap().name, "ucp-setup.exe");
    }
}
