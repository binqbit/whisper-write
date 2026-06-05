use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "linux")]
use std::fs::{File, OpenOptions};
#[cfg(target_os = "linux")]
use std::io::{Seek, SeekFrom, Write};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::{SystemTime, UNIX_EPOCH};

pub fn maybe_daemonize(enable: bool) -> Result<()> {
    if !enable {
        return Ok(());
    }

    daemonize_inner()
}

#[cfg(target_os = "linux")]
fn daemonize_inner() -> Result<()> {
    use daemonize::Daemonize;

    let working_directory =
        std::env::current_dir().context("Failed to determine daemon working directory")?;
    let log_path = working_directory.join("whisper-write.log");
    let pid_path = working_directory.join("whisper-write.pid");
    let mut pid_file = acquire_pid_lock(&pid_path)?;
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Failed to open daemon log at {}", log_path.display()))?;
    let stdout = log_file
        .try_clone()
        .with_context(|| format!("Failed to clone daemon log at {}", log_path.display()))?;

    writeln!(
        log_file,
        "\n[{}] starting whisper-write daemon",
        unix_timestamp()
    )
    .ok();

    let daemonize = Daemonize::new()
        .working_directory(&working_directory)
        .stdout(stdout)
        .stderr(log_file);

    daemonize
        .start()
        .with_context(|| format!("Failed to start daemon process; see {}", log_path.display()))?;

    write_pid(&mut pid_file, &pid_path)?;
    std::mem::forget(pid_file);
    Ok(())
}

#[cfg(target_os = "linux")]
fn acquire_pid_lock(path: &PathBuf) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("Failed to open pid file at {}", path.display()))?;

    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        return Ok(file);
    }

    let err = std::io::Error::last_os_error();
    let is_locked = matches!(
        err.raw_os_error(),
        Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN
    );
    if is_locked {
        return Err(anyhow!(
            "Another whisper-write daemon is already running; pid file is {}",
            path.display()
        ));
    }

    Err(err).with_context(|| format!("Failed to lock pid file at {}", path.display()))
}

#[cfg(target_os = "linux")]
fn write_pid(file: &mut File, path: &PathBuf) -> Result<()> {
    file.set_len(0)
        .with_context(|| format!("Failed to truncate pid file at {}", path.display()))?;
    file.seek(SeekFrom::Start(0))
        .with_context(|| format!("Failed to seek pid file at {}", path.display()))?;
    writeln!(file, "{}", std::process::id())
        .with_context(|| format!("Failed to write pid file at {}", path.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush pid file at {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
fn daemonize_inner() -> Result<()> {
    Err(anyhow::anyhow!("--daemon is only supported on Linux"))
}
