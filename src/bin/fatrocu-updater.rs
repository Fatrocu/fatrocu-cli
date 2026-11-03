use anyhow::{Context, Result};
use fatrocu_core::update::{Stage, install};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() {
    if let Err(e) = run() {
        eprintln!("Update failed: {e:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        (2..=3).contains(&args.len()),
        "Usage: fatrocu-updater STAGE_JSON INSTALLATION [RELAUNCH]"
    );
    let stage: Stage = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let installation = PathBuf::from(&args[1]);
    let start = Instant::now();
    let result = loop {
        match install(&stage, &installation) {
            Ok(()) => break Ok(()),
            Err(e) if start.elapsed() > Duration::from_secs(60) => break Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    };
    if let Some(relaunch) = args.get(2) {
        let path = PathBuf::from(relaunch);
        anyhow::ensure!(
            path.parent() == Some(installation.as_path()),
            "Relaunch must be in the installation directory"
        );
        let mut command = std::process::Command::new(path);
        if result.is_err() {
            command.arg("--update-failed");
        }
        command
            .spawn()
            .context("Update installed, but relaunch failed")?;
    }
    result
}
