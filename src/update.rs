//! Stable GitHub Releases, digest checks and pinned publisher verification.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub body: Option<String>,
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    etag: Option<String>,
    release: Release,
    checked_at: i64,
}
pub fn newer(release: &Release, current: &str) -> Result<bool> {
    ensure!(
        !release.draft && !release.prerelease,
        "Only published stable releases are supported"
    );
    let version = semver::Version::parse(release.tag_name.trim_start_matches('v'))?;
    ensure!(version.pre.is_empty(), "Prerelease excluded");
    Ok(version > semver::Version::parse(current)?)
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("Fatrocu/3.1 release-check")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()?)
}
pub fn check(repository: &str, cache_dir: &Path, manual: bool) -> Result<Release> {
    ensure!(
        matches!(repository, "Fatrocu" | "fatrocu-cli"),
        "Unsupported release repository"
    );
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(format!("release-{repository}.json"));
    let cache = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Cache>(&b).ok());
    if !manual
        && cache
            .as_ref()
            .is_some_and(|c| chrono::Utc::now().timestamp() - c.checked_at < 21600)
    {
        return Ok(cache.unwrap().release);
    }
    let mut request = client()?.get(format!(
        "https://api.github.com/repos/Fatrocu/{repository}/releases/latest"
    ));
    if let Some(etag) = cache.as_ref().and_then(|c| c.etag.as_ref()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = request.send()?;
    let next = if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        let mut c = cache.context("Missing release cache")?;
        c.checked_at = chrono::Utc::now().timestamp();
        c
    } else {
        let response = response.error_for_status()?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        let release: Release = response.json()?;
        ensure!(
            !release.draft && !release.prerelease,
            "No stable release available"
        );
        Cache {
            etag,
            release,
            checked_at: chrono::Utc::now().timestamp(),
        }
    };
    let mut temp = tempfile::NamedTempFile::new_in(cache_dir)?;
    serde_json::to_writer(temp.as_file_mut(), &next)?;
    temp.persist(path)?;
    Ok(next.release)
}
pub fn artifact_name(desktop: bool) -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    ensure!(
        matches!(os, "windows" | "linux") && matches!(arch, "x86_64" | "aarch64"),
        "Unsupported update platform"
    );
    ensure!(
        !desktop || os == "windows",
        "Desktop updates require Windows"
    );
    Ok(format!(
        "{}-{os}-{arch}.zip",
        if desktop {
            "fatrocu-desktop"
        } else {
            "fatrocu-cli"
        }
    ))
}
pub fn verify_digest(bytes: &[u8], digest: &str) -> Result<()> {
    let expected = digest
        .strip_prefix("sha256:")
        .context("Release asset has no published SHA-256 digest")?;
    ensure!(
        expected.len() == 64 && hex::encode(Sha256::digest(bytes)).eq_ignore_ascii_case(expected),
        "Release SHA-256 mismatch"
    );
    Ok(())
}
fn download(asset: &Asset) -> Result<Vec<u8>> {
    let url = reqwest::Url::parse(&asset.browser_download_url)?;
    ensure!(
        url.scheme() == "https" && url.host_str() == Some("github.com"),
        "Release asset must come from GitHub over HTTPS"
    );
    let client = reqwest::blocking::Client::builder()
        .user_agent("Fatrocu updater")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(300))
        .build()?;
    let response = client.get(url).send()?.error_for_status()?;
    let mut bytes = vec![];
    response
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 512 * 1024 * 1024,
        "Release artifact too large"
    );
    Ok(bytes)
}
pub fn verify_publisher(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let trusted=std::env::var("FATROCU_TRUSTED_PUBLISHER_SHA256").context("Set the confirmed release publisher certificate SHA-256 fingerprint before installing updates")?;
        ensure!(
            trusted.len() == 64 && trusted.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid publisher fingerprint"
        );
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("verify.ps1");
        std::fs::write(
            &script,
            "param([string]$Artifact)\n$s = Get-AuthenticodeSignature -LiteralPath $Artifact\nif ($s.Status -ne 'Valid' -or $null -eq $s.SignerCertificate) { exit 2 }\n$h = [System.Security.Cryptography.SHA256]::Create()\n[System.BitConverter]::ToString($h.ComputeHash($s.SignerCertificate.RawData)).Replace('-', '')\n",
        )?;
        let out = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(script)
            .arg(path)
            .output()?;
        ensure!(
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .eq_ignore_ascii_case(&trusted),
            "Executable publisher signature does not match trusted identity"
        );
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        anyhow::bail!("Use signed archive verification on Linux")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub directory: PathBuf,
    pub files: Vec<String>,
}
pub fn launch_helper(stage: &Stage, installation: &Path, relaunch: Option<&Path>) -> Result<()> {
    let root = stage
        .directory
        .parent()
        .context("Invalid stage directory")?;
    let source = installation.join("fatrocu-updater.exe");
    verify_publisher(&source)?;
    let helper = root.join("fatrocu-updater-running.exe");
    std::fs::copy(source, &helper)?;
    verify_publisher(&helper)?;
    let manifest = root.join("stage.json");
    std::fs::write(&manifest, serde_json::to_vec(stage)?)?;
    let mut command = std::process::Command::new(helper);
    command.arg(manifest).arg(installation);
    if let Some(path) = relaunch {
        command.arg(path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    Ok(())
}
pub fn stage(release: &Release, desktop: bool) -> Result<Stage> {
    let name = artifact_name(desktop)?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .context("No matching release artifact")?;
    let bytes = download(asset)?;
    verify_digest(
        &bytes,
        asset
            .digest
            .as_deref()
            .context("Missing published artifact digest")?,
    )?;
    let dir = tempfile::tempdir()?;
    #[cfg(not(windows))]
    {
        let key = std::env::var("FATROCU_TRUSTED_MINISIGN_KEY")
            .context("Set the confirmed release minisign public key")?;
        let sig = release
            .assets
            .iter()
            .find(|a| a.name == format!("{name}.minisig"))
            .context("Missing archive signature")?;
        let archive = dir.path().join("archive.zip");
        let signature = dir.path().join("archive.zip.minisig");
        std::fs::write(&archive, &bytes)?;
        std::fs::write(&signature, download(sig)?)?;
        ensure!(
            std::process::Command::new("minisign")
                .args(["-V", "-P", &key, "-m"])
                .arg(archive)
                .arg("-x")
                .arg(signature)
                .status()?
                .success(),
            "Archive signature rejected"
        );
    }
    let payload = dir.path().join("payload");
    std::fs::create_dir(&payload)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut files = vec![];
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let path = f.enclosed_name().context("Unsafe archive path")?;
        ensure!(
            path.components().count() == 1 && !f.is_dir(),
            "Update archive must contain flat files"
        );
        let name = path.to_str().context("Non-UTF8 archive path")?.to_owned();
        ensure!(!files.contains(&name), "Duplicate update file");
        expanded = expanded.checked_add(f.size()).context("Archive overflow")?;
        ensure!(expanded <= 1024 * 1024 * 1024, "Expanded release too large");
        #[cfg(windows)]
        ensure!(
            name.ends_with(".exe") || name.ends_with(".dll"),
            "Unexpected Windows update file"
        );
        let dest = payload.join(&name);
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)?;
        std::io::copy(&mut f, &mut out)?;
        out.sync_all()?;
        #[cfg(windows)]
        verify_publisher(&dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
        }
        files.push(name);
    }
    let binary = if desktop {
        "fatrocu.exe"
    } else if cfg!(windows) {
        "fatrocu-cli.exe"
    } else {
        "fatrocu-cli"
    };
    ensure!(
        files.iter().any(|f| f == binary),
        "Missing application binary"
    );
    if cfg!(windows) {
        ensure!(
            files.iter().any(|f| f == "fatrocu-updater.exe"),
            "Desktop update must include its helper"
        );
    }
    let root = dir.keep();
    Ok(Stage {
        directory: root.join("payload"),
        files,
    })
}
/// Install staged files with rollback. Caller must have closed the desktop first.
pub fn install(stage: &Stage, installation: &Path) -> Result<()> {
    install_checked(stage, installation, |source| {
        #[cfg(windows)]
        verify_publisher(source)?;
        let _ = source;
        Ok(())
    })
}
fn install_checked(
    stage: &Stage,
    installation: &Path,
    verify: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    ensure!(installation.is_dir(), "Installation directory is missing");
    let backup = tempfile::Builder::new()
        .prefix("fatrocu-rollback-")
        .tempdir_in(installation)?;
    let mut replaced = vec![];
    let result = (|| -> Result<()> {
        for name in &stage.files {
            ensure!(
                Path::new(name).components().count() == 1
                    && Path::new(name)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "Invalid stage file"
            );
            let source = stage.directory.join(name);
            ensure!(source.is_file(), "Missing staged file");
            verify(&source)?;
            let target = installation.join(name);
            if target.exists() {
                std::fs::rename(&target, backup.path().join(name))?;
            }
            replaced.push(name.clone());
            let mut temp = tempfile::NamedTempFile::new_in(installation)?;
            std::io::copy(&mut std::fs::File::open(source)?, temp.as_file_mut())?;
            temp.as_file().sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755))?;
            }
            temp.persist(&target)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let backup_path = backup.keep();
        for name in replaced.iter().rev() {
            let target = installation.join(name);
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            let previous = backup_path.join(name);
            if previous.exists() {
                std::fs::rename(previous, &target).with_context(|| {
                    format!(
                        "Rollback failed; recover files from {}",
                        backup_path.display()
                    )
                })?;
            }
        }
        let _ = std::fs::remove_dir(backup_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_install_restores_replaced_files() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("installed");
        let staged = root.path().join("staged");
        std::fs::create_dir(&installed).unwrap();
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(installed.join("application.exe"), "old").unwrap();
        std::fs::write(staged.join("application.exe"), "new").unwrap();
        let stage = Stage {
            directory: staged,
            files: vec!["application.exe".into(), "missing.exe".into()],
        };
        assert!(install_checked(&stage, &installed, |_| Ok(())).is_err());
        assert_eq!(
            std::fs::read_to_string(installed.join("application.exe")).unwrap(),
            "old"
        );
    }
    #[test]
    fn rejected_publisher_leaves_installation_untouched() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("installed");
        let staged = root.path().join("staged");
        std::fs::create_dir(&installed).unwrap();
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(installed.join("application.exe"), "old").unwrap();
        std::fs::write(staged.join("application.exe"), "untrusted").unwrap();
        let stage = Stage {
            directory: staged,
            files: vec!["application.exe".into()],
        };
        assert!(
            install_checked(&stage, &installed, |_| anyhow::bail!("Untrusted publisher")).is_err()
        );
        assert_eq!(
            std::fs::read_to_string(installed.join("application.exe")).unwrap(),
            "old"
        );
    }
}
