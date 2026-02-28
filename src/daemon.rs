use anyhow::{Context, Result};
pub fn maybe_daemonize(enable: bool) -> Result<()> {
    if !enable {
        return Ok(());
    }

    daemonize_inner()
}

#[cfg(target_os = "linux")]
fn daemonize_inner() -> Result<()> {
    use daemonize::Daemonize;

    let daemonize = Daemonize::new().working_directory("/");

    daemonize
        .start()
        .context("Failed to start daemon process")?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn daemonize_inner() -> Result<()> {
    Err(anyhow::anyhow!("--daemon is only supported on Linux"))
}
