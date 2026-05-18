use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{init, project};

const REPO: &str = "av/facts";
const BINARY: &str = "facts";
const VERSION: &str = env!("CARGO_PKG_VERSION");

enum InstallMethod {
    Npm,
    Pipx,
    Direct,
}

pub fn run() -> Result<()> {
    let exe = std::env::current_exe().context("cannot determine binary path")?;
    let method = detect_install_method(&exe);

    match method {
        InstallMethod::Npm => update_npm()?,
        InstallMethod::Pipx => update_pipx()?,
        InstallMethod::Direct => update_direct(&exe)?,
    }

    if let Ok(root) = project::find_project_root() {
        if root.join(".facts").is_file() {
            println!();
            init::run(None)?;
        }
    }

    Ok(())
}

fn detect_install_method(exe: &Path) -> InstallMethod {
    let path_str = exe.to_string_lossy();
    if path_str.contains("node_modules") || path_str.contains("/npm/") || path_str.contains("/nvm/")
    {
        return InstallMethod::Npm;
    }
    if path_str.contains("pipx/venvs") || path_str.contains("site-packages") {
        return InstallMethod::Pipx;
    }
    InstallMethod::Direct
}

fn update_npm() -> Result<()> {
    println!("detected npm install, running: npm update -g @avcodes/facts");
    let status = Command::new("npm")
        .args(["update", "-g", "@avcodes/facts"])
        .status()
        .context("failed to run npm")?;
    if !status.success() {
        bail!("npm update failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

fn update_pipx() -> Result<()> {
    println!("detected pipx install, running: pipx upgrade facts-cli");
    let status = Command::new("pipx")
        .args(["upgrade", "facts-cli"])
        .status()
        .context("failed to run pipx")?;
    if !status.success() {
        bail!("pipx upgrade failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

fn update_direct(exe: &Path) -> Result<()> {
    let latest_tag = get_latest_version()?;
    let latest_version = latest_tag.strip_prefix('v').unwrap_or(&latest_tag);

    if latest_version == VERSION {
        println!("already up to date (v{})", VERSION);
        return Ok(());
    }

    let target = detect_platform()?;
    let url = format!(
        "https://github.com/{REPO}/releases/download/{latest_tag}/{BINARY}-{target}.tar.gz"
    );

    println!("updating v{VERSION} → v{latest_version}...");

    let tmpdir = tempdir()?;
    let archive = tmpdir.join("download.tar.gz");

    download(&url, &archive)?;
    extract(&archive, &tmpdir)?;

    let new_binary = tmpdir.join(BINARY);
    if !new_binary.exists() {
        bail!("archive did not contain '{BINARY}' binary");
    }

    replace_binary(&new_binary, exe)?;
    println!("updated v{VERSION} → v{latest_version}");

    std::fs::remove_dir_all(&tmpdir).ok();
    Ok(())
}

fn detect_platform() -> Result<String> {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        bail!("self-update is not supported on Windows; use npm or pip to update");
    } else {
        bail!("unsupported operating system");
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        bail!("unsupported architecture");
    };

    Ok(format!("{os}-{arch}"))
}

fn find_downloader() -> Result<&'static str> {
    for cmd in &["curl", "wget"] {
        let result = Command::new(cmd)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if result.map(|s| s.success()).unwrap_or(false) {
            return Ok(cmd);
        }
    }
    bail!("curl or wget is required for self-update");
}

fn get_latest_version() -> Result<String> {
    let cmd = find_downloader()?;
    let output = match cmd {
        "curl" => Command::new("curl")
            .args([
                "-fsSL",
                &format!("https://api.github.com/repos/{REPO}/releases/latest"),
            ])
            .output()
            .context("failed to run curl")?,
        "wget" => Command::new("wget")
            .args([
                "-qO-",
                &format!("https://api.github.com/repos/{REPO}/releases/latest"),
            ])
            .output()
            .context("failed to run wget")?,
        _ => unreachable!(),
    };

    if !output.status.success() {
        bail!("failed to fetch latest version from GitHub");
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let tag = body
        .lines()
        .find(|l| l.contains("\"tag_name\""))
        .and_then(|l| {
            let after_key = &l[l.find("tag_name")? + "tag_name".len()..];
            let after_colon = &after_key[after_key.find(':')? + 1..];
            let open = after_colon.find('"')? + 1;
            let rest = &after_colon[open..];
            let close = rest.find('"')?;
            Some(rest[..close].to_string())
        })
        .context("could not parse tag_name from GitHub API response")?;

    Ok(tag)
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let cmd = find_downloader()?;
    let status = match cmd {
        "curl" => Command::new("curl")
            .args(["-fsSL", url, "-o"])
            .arg(dest)
            .status()
            .context("failed to run curl")?,
        "wget" => Command::new("wget")
            .args(["-qO"])
            .arg(dest)
            .arg(url)
            .status()
            .context("failed to run wget")?,
        _ => unreachable!(),
    };

    if !status.success() {
        bail!("download failed: {url}");
    }
    Ok(())
}

fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let status = Command::new("tar")
        .args(["xzf"])
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .status()
        .context("failed to run tar")?;
    if !status.success() {
        bail!("failed to extract archive");
    }
    Ok(())
}

fn replace_binary(src: &Path, dest: &Path) -> Result<()> {
    // Atomic-ish replacement: copy to temp next to dest, then rename.
    // rename() across filesystems fails, so we write to the same dir.
    let dest_dir = dest.parent().unwrap_or(Path::new("."));
    let tmp_dest = dest_dir.join(".facts.update.tmp");

    std::fs::copy(src, &tmp_dest)
        .with_context(|| format!("failed to copy new binary to {}", tmp_dest.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_dest, std::fs::Permissions::from_mode(0o755))
            .context("failed to set permissions")?;
    }

    std::fs::rename(&tmp_dest, dest)
        .with_context(|| format!("failed to replace binary at {}", dest.display()))?;

    Ok(())
}

fn tempdir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("facts-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).context("failed to create temp directory")?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_npm() {
        let p = Path::new("/usr/local/lib/node_modules/@avcodes/facts/bin/facts");
        assert!(matches!(detect_install_method(p), InstallMethod::Npm));
    }

    #[test]
    fn test_detect_npm_nvm() {
        let p = Path::new(
            "/home/user/.nvm/versions/node/v20/lib/node_modules/@avcodes/facts/bin/facts",
        );
        assert!(matches!(detect_install_method(p), InstallMethod::Npm));
    }

    #[test]
    fn test_detect_pipx() {
        let p = Path::new("/home/user/.local/pipx/venvs/facts-cli/bin/facts");
        assert!(matches!(detect_install_method(p), InstallMethod::Pipx));
    }

    #[test]
    fn test_detect_pipx_share_path() {
        let p = Path::new("/root/.local/share/pipx/venvs/facts-cli/bin/facts");
        assert!(matches!(detect_install_method(p), InstallMethod::Pipx));
    }

    #[test]
    fn test_detect_direct_usr_local() {
        let p = Path::new("/usr/local/bin/facts");
        assert!(matches!(detect_install_method(p), InstallMethod::Direct));
    }

    #[test]
    fn test_detect_direct_cargo() {
        let p = Path::new("/home/user/.cargo/bin/facts");
        assert!(matches!(detect_install_method(p), InstallMethod::Direct));
    }

    #[test]
    fn test_detect_platform() {
        let target = detect_platform().unwrap();
        assert!(
            target.contains("linux") || target.contains("darwin"),
            "unexpected target: {target}"
        );
        assert!(
            target.contains("amd64") || target.contains("arm64"),
            "unexpected target: {target}"
        );
    }

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }
}
