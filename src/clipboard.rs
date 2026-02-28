use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

pub struct Clipboard {
    inner: arboard::Clipboard,
}

impl Clipboard {
    pub fn new() -> Result<Self> {
        let inner = arboard::Clipboard::new().context("Failed to initialize clipboard")?;
        Ok(Self { inner })
    }

    pub fn set_text(&mut self, text: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        if self.set_text_linux(text).is_ok() {
            return Ok(());
        }

        self.inner
            .set_text(text.to_string())
            .context("Failed to set clipboard text")
    }

    pub fn get_text(&mut self) -> Result<String> {
        #[cfg(target_os = "linux")]
        if let Ok(text) = self.get_text_linux() {
            return Ok(text);
        }

        match self.inner.get_text() {
            Ok(text) => Ok(text),
            // Empty or non-text clipboard should not break output flow.
            Err(_) => Ok(String::new()),
        }
    }
}

#[cfg(target_os = "linux")]
impl Clipboard {
    fn set_text_linux(&mut self, text: &str) -> Result<()> {
        for backend in available_write_backends() {
            let result = match backend {
                LinuxClipboardBackend::WlClipboard => write_text_to_command("wl-copy", &[], text),
                LinuxClipboardBackend::KdeKlipper => set_text_with_klipper(text),
                LinuxClipboardBackend::XclipClipboard => {
                    write_text_to_command("xclip", &["-selection", "clipboard"], text)
                }
                LinuxClipboardBackend::XselClipboard => {
                    write_text_to_command("xsel", &["--clipboard", "--input"], text)
                }
                LinuxClipboardBackend::WlPrimary
                | LinuxClipboardBackend::XclipPrimary
                | LinuxClipboardBackend::XselPrimary => {
                    continue;
                }
            };

            if result.is_ok() {
                return Ok(());
            }
        }

        Err(anyhow!("No Linux system clipboard backend is available"))
    }

    fn get_text_linux(&mut self) -> Result<String> {
        for backend in available_read_backends() {
            let result = match backend {
                LinuxClipboardBackend::WlClipboard => {
                    read_text_from_command("wl-paste", &["--no-newline"])
                }
                LinuxClipboardBackend::KdeKlipper => get_text_from_klipper(),
                LinuxClipboardBackend::XclipClipboard => {
                    read_text_from_command("xclip", &["-selection", "clipboard", "-o"])
                }
                LinuxClipboardBackend::XselClipboard => {
                    read_text_from_command("xsel", &["--clipboard", "--output"])
                }
                LinuxClipboardBackend::WlPrimary => {
                    read_text_from_command("wl-paste", &["--primary", "--no-newline"])
                }
                LinuxClipboardBackend::XclipPrimary => {
                    read_text_from_command("xclip", &["-selection", "primary", "-o"])
                }
                LinuxClipboardBackend::XselPrimary => {
                    read_text_from_command("xsel", &["--primary", "--output"])
                }
            };

            if let Ok(text) = result {
                // Some providers report success but return empty payload for certain sources.
                if !text.trim().is_empty() {
                    return Ok(text);
                }
            }
        }

        Err(anyhow!("No Linux system clipboard backend is available"))
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
enum LinuxClipboardBackend {
    WlClipboard,
    KdeKlipper,
    XclipClipboard,
    XselClipboard,
    WlPrimary,
    XclipPrimary,
    XselPrimary,
}

#[cfg(target_os = "linux")]
fn available_write_backends() -> Vec<LinuxClipboardBackend> {
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut backends = Vec::new();

    if is_wayland && command_exists("wl-copy") {
        backends.push(LinuxClipboardBackend::WlClipboard);
    }
    if command_exists("qdbus") {
        backends.push(LinuxClipboardBackend::KdeKlipper);
    }
    if command_exists("xclip") {
        backends.push(LinuxClipboardBackend::XclipClipboard);
    }
    if command_exists("xsel") {
        backends.push(LinuxClipboardBackend::XselClipboard);
    }

    backends
}

#[cfg(target_os = "linux")]
fn available_read_backends() -> Vec<LinuxClipboardBackend> {
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut backends = Vec::new();

    if is_wayland && command_exists("wl-paste") {
        backends.push(LinuxClipboardBackend::WlClipboard);
    }
    if command_exists("qdbus") {
        backends.push(LinuxClipboardBackend::KdeKlipper);
    }
    if command_exists("xclip") {
        backends.push(LinuxClipboardBackend::XclipClipboard);
    }
    if command_exists("xsel") {
        backends.push(LinuxClipboardBackend::XselClipboard);
    }

    // Some web "Copy" actions may land in selection buffers.
    if is_wayland && command_exists("wl-paste") {
        backends.push(LinuxClipboardBackend::WlPrimary);
    }
    if command_exists("xclip") {
        backends.push(LinuxClipboardBackend::XclipPrimary);
    }
    if command_exists("xsel") {
        backends.push(LinuxClipboardBackend::XselPrimary);
    }

    backends
}

#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn write_text_to_command(command: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to spawn {command}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .with_context(|| format!("Failed to write clipboard data to {command}"))?;
    }

    let status = child
        .wait()
        .with_context(|| format!("Failed to wait for {command}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{command} exited with status {status}"))
    }
}

#[cfg(target_os = "linux")]
fn read_text_from_command(command: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run {command}"))?;

    if !output.status.success() {
        return Err(anyhow!("{command} exited with status {}", output.status));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "linux")]
fn set_text_with_klipper(text: &str) -> Result<()> {
    let status = Command::new("qdbus")
        .args([
            "org.kde.klipper",
            "/klipper",
            "org.kde.klipper.klipper.setClipboardContents",
            text,
        ])
        .status()
        .context("Failed to run qdbus for Klipper setClipboardContents")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "qdbus setClipboardContents exited with status {status}"
        ))
    }
}

#[cfg(target_os = "linux")]
fn get_text_from_klipper() -> Result<String> {
    let text = read_text_from_command(
        "qdbus",
        &[
            "org.kde.klipper",
            "/klipper",
            "org.kde.klipper.klipper.getClipboardContents",
        ],
    )?;
    Ok(text.trim_end_matches('\n').to_string())
}
