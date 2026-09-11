use std::fs::{self, File};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::UpdateArgs;

const REPOSITORY: &str = "wendaining/recall";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest: String,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    let release = latest_release()?;
    if !is_newer(&release.tag_name, env!("CARGO_PKG_VERSION"))? {
        println!("recall {} is up to date", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.check {
        println!(
            "update available: {} -> {}",
            env!("CARGO_PKG_VERSION"),
            release.tag_name
        );
        return Ok(());
    }

    let target = target()?;
    let archive_name = archive_name(&release.tag_name, target);
    let archive = release
        .assets
        .iter()
        .find(|asset| asset.name == archive_name)
        .context("latest release does not include an archive for this platform")?;
    let checksums = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .context("latest release does not include SHA256SUMS")?;
    let expected = checksum_for(
        &download_text(&checksums.browser_download_url)?,
        &archive_name,
    )?;

    let executable = std::env::current_exe().context("locating the running recall binary")?;
    let parent = executable
        .parent()
        .context("locating the binary directory")?;
    let stage = parent.join(format!(".recall-update-{}", std::process::id()));
    if stage.exists() {
        fs::remove_dir_all(&stage).with_context(|| format!("clearing {}", stage.display()))?;
    }
    fs::create_dir(&stage).with_context(|| format!("creating {}", stage.display()))?;
    let result = install_release(&stage, archive, &expected, &executable);
    let _ = fs::remove_dir_all(&stage);
    result
}

pub fn due_notice() -> Option<String> {
    let cache_path = cache_path()?;
    if let Some(cache) = read_cache(&cache_path)
        && now_secs().saturating_sub(cache.checked_at) < CHECK_INTERVAL.as_secs()
    {
        return newer_notice(&cache.latest);
    }
    let release = latest_release().ok()?;
    let cache = UpdateCache {
        checked_at: now_secs(),
        latest: release.tag_name,
    };
    let _ = write_cache(&cache_path, &cache);
    newer_notice(&cache.latest)
}

fn install_release(stage: &Path, archive: &Asset, expected: &str, executable: &Path) -> Result<()> {
    let archive_path = stage.join(&archive.name);
    let actual = download_with_progress(&archive.browser_download_url, &archive_path)?;
    if actual != expected {
        bail!("checksum mismatch for {}", archive.name);
    }
    eprintln!("checksum verified");

    let replacement = stage.join(executable_name());
    extract_binary(&archive_path, &replacement)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755))?;
    }
    replace_binary(&replacement, executable)?;
    println!(
        "updated recall to {}",
        latest_version_from_archive(&archive.name)?
    );
    Ok(())
}

fn latest_release() -> Result<Release> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let release: Release = client()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .context("checking the latest recall release")?
        .error_for_status()
        .context("checking the latest recall release")?
        .json()
        .context("reading the latest recall release")?;
    if release.draft || release.prerelease {
        bail!("latest release is not a stable release");
    }
    parse_version(&release.tag_name)?;
    Ok(release)
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("recall-updater")
        .build()
        .expect("valid HTTP client configuration")
}

fn download_text(url: &str) -> Result<String> {
    client()
        .get(url)
        .send()
        .with_context(|| format!("downloading {url}"))?
        .error_for_status()
        .with_context(|| format!("downloading {url}"))?
        .text()
        .with_context(|| format!("reading {url}"))
}

fn download_with_progress(url: &str, destination: &Path) -> Result<String> {
    let mut response = client()
        .get(url)
        .send()
        .with_context(|| format!("downloading {url}"))?
        .error_for_status()
        .with_context(|| format!("downloading {url}"))?;
    let total = response.content_length();
    let mut file =
        File::create(destination).with_context(|| format!("creating {}", destination.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .context("reading update download")?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .context("writing update download")?;
        hasher.update(&buffer[..count]);
        bytes += count as u64;
        draw_progress(bytes, total);
    }
    clear_progress();
    Ok(format!("{:x}", hasher.finalize()))
}

fn draw_progress(bytes: u64, total: Option<u64>) {
    if !io::stderr().is_terminal() {
        return;
    }
    let progress = total
        .map(|total| bytes.saturating_mul(100) / total)
        .unwrap_or(0);
    let filled = (progress / 5).min(20) as usize;
    eprint!(
        "\r\x1b[36mupdate\x1b[0m [{}{}] {:>3}% {}",
        "█".repeat(filled),
        "·".repeat(20 - filled),
        progress,
        format_bytes(bytes)
    );
    if let Some(total) = total {
        eprint!(" / {}", format_bytes(total));
    }
    let _ = io::stderr().flush();
}

fn clear_progress() {
    if io::stderr().is_terminal() {
        eprintln!();
    }
}

fn extract_binary(archive: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let file = File::open(archive)?;
        let mut zip = zip::ZipArchive::new(file).context("opening update archive")?;
        let mut entry = zip
            .by_name(executable_name())
            .context("archive does not contain recall.exe")?;
        let mut output = File::create(destination)?;
        io::copy(&mut entry, &mut output)?;
    }
    #[cfg(not(windows))]
    {
        let file = File::open(archive)?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut tar = tar::Archive::new(decoder);
        let mut found = false;
        for entry in tar.entries().context("opening update archive")? {
            let mut entry = entry?;
            if entry
                .path()?
                .file_name()
                .is_some_and(|name| name == executable_name())
            {
                let mut output = File::create(destination)?;
                io::copy(&mut entry, &mut output)?;
                found = true;
                break;
            }
        }
        if !found {
            bail!("archive does not contain recall");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn replace_binary(replacement: &Path, executable: &Path) -> Result<()> {
    fs::rename(replacement, executable)
        .with_context(|| format!("replacing {}", executable.display()))
}

#[cfg(windows)]
fn replace_binary(replacement: &Path, executable: &Path) -> Result<()> {
    let pending = executable.with_extension("exe.recall-new");
    fs::rename(replacement, &pending)?;
    let script = std::env::temp_dir().join(format!("recall-update-{}.cmd", std::process::id()));
    let pid = std::process::id();
    fs::write(
        &script,
        format!(
            "@echo off\r\n:wait\r\ntasklist /FI \"PID eq {pid}\" /NH | find \"{pid}\" >nul\r\nif not errorlevel 1 (\r\n  timeout /t 1 /nobreak >nul\r\n  goto wait\r\n)\r\nmove /Y \"{}\" \"{}\" >nul\r\ndel \"%~f0\"\r\n",
            pending.display(),
            executable.display()
        ),
    )?;
    std::process::Command::new("cmd")
        .args(["/C", &script.to_string_lossy()])
        .spawn()
        .context("starting the Windows update helper")?;
    println!("updated recall; restart the terminal to use the new version");
    Ok(())
}

fn checksum_for(text: &str, archive: &str) -> Result<String> {
    text.lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            (name.trim_start_matches('*').trim() == archive).then(|| hash.to_ascii_lowercase())
        })
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .context("SHA256SUMS does not contain the update archive")
}

fn archive_name(tag: &str, target: &str) -> String {
    format!(
        "recall-{tag}-{target}.{}",
        if cfg!(windows) { "zip" } else { "tar.gz" }
    )
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "recall.exe"
    } else {
        "recall"
    }
}

fn target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, arch) => bail!("unsupported update platform: {os}/{arch}"),
    }
}

fn parse_version(version: &str) -> Result<Vec<u64>> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("release version is not valid semver")?;
    if parts.len() != 3 {
        bail!("release version is not valid semver");
    }
    Ok(parts)
}

fn is_newer(candidate: &str, current: &str) -> Result<bool> {
    Ok(parse_version(candidate)? > parse_version(current)?)
}

fn latest_version_from_archive(name: &str) -> Result<&str> {
    name.strip_prefix("recall-")
        .and_then(|name| name.split('-').next())
        .context("reading update version")
}

fn cache_path() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("recall").join("update.json"))
}

fn read_cache(path: &Path) -> Option<UpdateCache> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn write_cache(path: &Path, cache: &UpdateCache) -> Result<()> {
    let directory = path.parent().context("locating update cache directory")?;
    fs::create_dir_all(directory)?;
    fs::write(path, serde_json::to_vec(cache)?)?;
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn newer_notice(latest: &str) -> Option<String> {
    is_newer(latest, env!("CARGO_PKG_VERSION"))
        .ok()
        .filter(|newer| *newer)
        .map(|_| format!("update available: {latest} · run recall update"))
}

fn format_bytes(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_stable_versions() {
        assert!(is_newer("v0.2.0", "0.1.0").unwrap());
        assert!(!is_newer("v0.1.0", "0.1.0").unwrap());
        assert!(!is_newer("v0.0.9", "0.1.0").unwrap());
    }

    #[test]
    fn rejects_non_stable_versions() {
        assert!(parse_version("v0.2.0-rc.1").is_err());
        assert!(parse_version("v0.2").is_err());
    }

    #[test]
    fn finds_checksum_for_named_archive() {
        let checksum = checksum_for(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789  recall-v1.2.3-aarch64-apple-darwin.tar.gz\n",
            "recall-v1.2.3-aarch64-apple-darwin.tar.gz",
        )
        .unwrap();
        assert_eq!(
            checksum,
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        );
    }
}
