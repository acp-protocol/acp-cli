//! @acp:module "Install Command"
//! @acp:summary "Plugin installation for ACP daemon and MCP server"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Installs ACP plugins (daemon, mcp) by downloading pre-built binaries
//! from GitHub releases.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use console::style;

/// Plugin installation targets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallTarget {
    Daemon,
    Mcp,
}

impl InstallTarget {
    /// GitHub repository name
    fn repo(&self) -> &'static str {
        match self {
            InstallTarget::Daemon => "acp-daemon",
            InstallTarget::Mcp => "acp-mcp",
        }
    }

    /// Binary name
    fn binary_name(&self) -> &'static str {
        match self {
            InstallTarget::Daemon => "acpd",
            InstallTarget::Mcp => "acp-mcp",
        }
    }

    /// Display name
    fn display_name(&self) -> &'static str {
        match self {
            InstallTarget::Daemon => "ACP Daemon",
            InstallTarget::Mcp => "ACP MCP Server",
        }
    }
}

impl std::str::FromStr for InstallTarget {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "daemon" | "acpd" => Ok(InstallTarget::Daemon),
            "mcp" | "acp-mcp" => Ok(InstallTarget::Mcp),
            _ => Err(format!("Unknown target: {}. Use 'daemon' or 'mcp'", s)),
        }
    }
}

/// Installation options
pub struct InstallOptions {
    pub targets: Vec<InstallTarget>,
    pub force: bool,
    pub version: Option<String>,
}

/// Detect current platform
fn detect_platform() -> Result<&'static str> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        _ => Err(anyhow!("Unsupported platform: {}-{}", os, arch)),
    }
}

/// Get installation directory
fn get_install_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".acp").join("bin"))
        .unwrap_or_else(|| PathBuf::from(".acp/bin"))
}

/// GitHub API response for release
#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// GitHub API response for asset
#[derive(Debug, serde::Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Fetch latest release info from GitHub
fn fetch_latest_release(repo: &str) -> Result<GitHubRelease> {
    let url = format!(
        "https://api.github.com/repos/acp-protocol/{}/releases/latest",
        repo
    );

    let response = ureq::get(&url)
        .set("User-Agent", "acp-cli")
        .call()
        .context("Failed to fetch release info")?;

    let release: GitHubRelease = response
        .into_json()
        .context("Failed to parse release info")?;

    Ok(release)
}

/// Fetch specific release info from GitHub
fn fetch_release(repo: &str, version: &str) -> Result<GitHubRelease> {
    let tag = if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{}", version)
    };

    let url = format!(
        "https://api.github.com/repos/acp-protocol/{}/releases/tags/{}",
        repo, tag
    );

    let response = ureq::get(&url)
        .set("User-Agent", "acp-cli")
        .call()
        .context("Failed to fetch release info")?;

    let release: GitHubRelease = response
        .into_json()
        .context("Failed to parse release info")?;

    Ok(release)
}

/// Find asset for the current platform
fn find_asset_for_platform<'a>(
    release: &'a GitHubRelease,
    platform: &str,
    binary_name: &str,
) -> Option<&'a GitHubAsset> {
    // Look for tar.gz or zip based on platform
    let ext = if platform.contains("windows") {
        ".zip"
    } else {
        ".tar.gz"
    };

    // Asset name format: {binary}-{platform}.{ext}
    let expected_name = format!("{}-{}{}", binary_name, platform, ext);

    release
        .assets
        .iter()
        .find(|a| a.name == expected_name)
        .or_else(|| {
            // Also try without binary name prefix (just platform)
            let alt_name = format!("{}{}", platform, ext);
            release.assets.iter().find(|a| a.name.contains(&alt_name))
        })
}

/// Download and extract binary
fn download_and_extract(
    url: &str,
    install_dir: &PathBuf,
    binary_name: &str,
    is_windows: bool,
) -> Result<PathBuf> {
    println!("  {} Downloading...", style("↓").blue());

    // Create install directory
    fs::create_dir_all(install_dir).context("Failed to create install directory")?;

    // Download to temp file
    let response = ureq::get(url)
        .set("User-Agent", "acp-cli")
        .call()
        .context("Failed to download")?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .context("Failed to read download")?;

    println!("  {} Extracting...", style("⚙").blue());

    // Extract binary
    let binary_path = if is_windows {
        extract_zip(&bytes, install_dir, binary_name)?
    } else {
        extract_tar_gz(&bytes, install_dir, binary_name)?
    };

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&binary_path)
            .context("Failed to get permissions")?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary_path, perms).context("Failed to set permissions")?;
    }

    Ok(binary_path)
}

/// Extract tar.gz archive
fn extract_tar_gz(data: &[u8], install_dir: &PathBuf, binary_name: &str) -> Result<PathBuf> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);

    let binary_path = install_dir.join(binary_name);

    for entry in archive.entries().context("Failed to read archive")? {
        let mut entry = entry.context("Failed to read entry")?;
        let entry_path = entry.path().context("Failed to get path")?;

        // Check if this is the binary we want
        let file_name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if file_name == binary_name || file_name == format!("{}.exe", binary_name) {
            let mut file = File::create(&binary_path).context("Failed to create file")?;
            io::copy(&mut entry, &mut file).context("Failed to extract")?;
            return Ok(binary_path);
        }
    }

    Err(anyhow!("Binary '{}' not found in archive", binary_name))
}

/// Extract zip archive
fn extract_zip(data: &[u8], install_dir: &PathBuf, binary_name: &str) -> Result<PathBuf> {
    use std::io::Cursor;
    use zip::ZipArchive;

    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).context("Failed to read zip")?;

    let binary_path = install_dir.join(format!("{}.exe", binary_name));

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("Failed to read entry")?;

        let file_name = file.name();

        // Check if this is the binary we want
        if file_name.ends_with(&format!("{}.exe", binary_name)) || file_name.ends_with(binary_name)
        {
            let mut outfile = File::create(&binary_path).context("Failed to create file")?;
            io::copy(&mut file, &mut outfile).context("Failed to extract")?;
            return Ok(binary_path);
        }
    }

    Err(anyhow!("Binary '{}' not found in zip", binary_name))
}

/// Check if binary already exists
fn check_existing(install_dir: &PathBuf, binary_name: &str, is_windows: bool) -> Option<PathBuf> {
    let name = if is_windows {
        format!("{}.exe", binary_name)
    } else {
        binary_name.to_string()
    };

    let path = install_dir.join(name);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Suggest PATH update
fn suggest_path_update(install_dir: &PathBuf) {
    let path_str = install_dir.display().to_string();

    // Check if already in PATH
    if let Ok(path) = std::env::var("PATH") {
        if path.contains(&path_str) {
            return;
        }
    }

    println!();
    println!(
        "{} Add the following to your shell profile:",
        style("Note:").yellow()
    );

    if cfg!(windows) {
        println!("  setx PATH \"%PATH%;{}\"", install_dir.display());
    } else {
        println!("  export PATH=\"$PATH:{}\"", install_dir.display());
    }
}

/// Execute the install command
pub fn execute_install(options: InstallOptions) -> Result<()> {
    let platform = detect_platform()?;
    let install_dir = get_install_dir();
    let is_windows = platform.contains("windows");

    println!(
        "{} Installing ACP plugins to {}",
        style("→").blue(),
        style(install_dir.display()).cyan()
    );
    println!("  Platform: {}", style(platform).dim());
    println!();

    let mut installed = Vec::new();

    for target in &options.targets {
        println!(
            "{} {}",
            style("Installing").green().bold(),
            style(target.display_name()).cyan()
        );

        // Check if already installed
        if let Some(existing) = check_existing(&install_dir, target.binary_name(), is_windows) {
            if !options.force {
                println!(
                    "  {} Already installed at {}",
                    style("✓").green(),
                    existing.display()
                );
                println!("  Use --force to reinstall");
                continue;
            }
            println!("  {} Reinstalling...", style("!").yellow());
        }

        // Fetch release info
        let release = if let Some(ref version) = options.version {
            fetch_release(target.repo(), version)?
        } else {
            fetch_latest_release(target.repo())?
        };

        println!("  Version: {}", style(&release.tag_name).dim());

        // Find asset for platform
        let asset = find_asset_for_platform(&release, platform, target.binary_name()).ok_or_else(
            || {
                anyhow!(
                    "No binary found for {} on {}. Available: {:?}",
                    target.display_name(),
                    platform,
                    release.assets.iter().map(|a| &a.name).collect::<Vec<_>>()
                )
            },
        )?;

        // Download and extract
        let binary_path = download_and_extract(
            &asset.browser_download_url,
            &install_dir,
            target.binary_name(),
            is_windows,
        )?;

        println!(
            "  {} Installed to {}",
            style("✓").green(),
            binary_path.display()
        );

        installed.push(target.display_name());
        println!();
    }

    if !installed.is_empty() {
        println!(
            "{} Successfully installed: {}",
            style("✓").green().bold(),
            installed.join(", ")
        );
        suggest_path_update(&install_dir);
    }

    Ok(())
}

/// List installed plugins
pub fn execute_list_installed() -> Result<()> {
    let install_dir = get_install_dir();

    println!(
        "{} Installed plugins in {}",
        style("→").blue(),
        style(install_dir.display()).cyan()
    );

    let is_windows = cfg!(windows);

    for target in [InstallTarget::Daemon, InstallTarget::Mcp] {
        if let Some(path) = check_existing(&install_dir, target.binary_name(), is_windows) {
            println!(
                "  {} {} ({})",
                style("✓").green(),
                target.display_name(),
                path.display()
            );
        } else {
            println!(
                "  {} {} (not installed)",
                style("✗").dim(),
                target.display_name()
            );
        }
    }

    Ok(())
}

/// Uninstall a plugin
pub fn execute_uninstall(targets: Vec<InstallTarget>) -> Result<()> {
    let install_dir = get_install_dir();
    let is_windows = cfg!(windows);

    for target in targets {
        let binary_name = if is_windows {
            format!("{}.exe", target.binary_name())
        } else {
            target.binary_name().to_string()
        };

        let path = install_dir.join(&binary_name);

        if path.exists() {
            fs::remove_file(&path)?;
            println!(
                "{} Uninstalled {}",
                style("✓").green(),
                target.display_name()
            );
        } else {
            println!(
                "{} {} is not installed",
                style("!").yellow(),
                target.display_name()
            );
        }
    }

    Ok(())
}
