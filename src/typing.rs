use anyhow::{anyhow, Result};
use enigo::{Enigo, Key, KeyboardControllable};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::clipboard::Clipboard;
use crate::config::OutputMode;

pub struct Typer {
    mode: OutputMode,
    enigo: Option<Enigo>,
    clipboard: Option<Clipboard>,
    restore_clipboard: bool,
}

impl Typer {
    pub fn new(mode: OutputMode, restore_clipboard: bool) -> Result<Self> {
        let enigo = Some(Enigo::new());
        let clipboard = if matches!(mode, OutputMode::Paste | OutputMode::Auto) {
            Some(Clipboard::new()?)
        } else {
            None
        };
        Ok(Self {
            mode,
            enigo,
            clipboard,
            restore_clipboard,
        })
    }

    pub fn type_text(&mut self, text: &str) -> Result<()> {
        if self.enigo.is_none() {
            return Ok(());
        }

        match self.mode {
            OutputMode::Auto => {
                if try_wtype(text).is_ok() {
                    return Ok(());
                }

                if let (Some(clipboard), Some(enigo)) =
                    (self.clipboard.as_mut(), self.enigo.as_mut())
                {
                    paste(clipboard, enigo, text, self.restore_clipboard)?;
                    return Ok(());
                }

                if let Some(enigo) = self.enigo.as_mut() {
                    enigo.key_sequence(text);
                }
            }
            OutputMode::Type => {
                if try_wtype(text).is_ok() {
                    return Ok(());
                }

                if cfg!(target_os = "linux") && needs_clipboard_fallback(text) {
                    if self.clipboard.is_none() {
                        self.clipboard = Some(Clipboard::new()?);
                    }

                    if let (Some(clipboard), Some(enigo)) =
                        (self.clipboard.as_mut(), self.enigo.as_mut())
                    {
                        paste(clipboard, enigo, text, self.restore_clipboard)?;
                        return Ok(());
                    }
                }

                if let Some(enigo) = self.enigo.as_mut() {
                    enigo.key_sequence(text);
                }
            }
            OutputMode::Paste => {
                if let (Some(clipboard), Some(enigo)) =
                    (self.clipboard.as_mut(), self.enigo.as_mut())
                {
                    paste(clipboard, enigo, text, self.restore_clipboard)?;
                }
            }
        }
        Ok(())
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn try_wtype(text: &str) -> Result<()> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !wayland || !command_exists("wtype") {
        return Err(anyhow!("wtype unavailable"));
    }

    let mut child = Command::new("wtype")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("wtype failed"))
    }
}

fn paste(
    clipboard: &mut Clipboard,
    enigo: &mut Enigo,
    text: &str,
    restore_clipboard: bool,
) -> Result<()> {
    if restore_clipboard {
        paste_with_restore(clipboard, enigo, text)?;
        return Ok(());
    }

    paste_without_restore(clipboard, enigo, text)?;
    Ok(())
}

fn paste_with_restore(clipboard: &mut Clipboard, enigo: &mut Enigo, text: &str) -> Result<()> {
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text)?;
    enigo.key_down(Key::Control);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Control);
    if let Some(previous) = previous {
        clipboard.set_text(&previous)?;
    }
    Ok(())
}

fn paste_without_restore(clipboard: &mut Clipboard, enigo: &mut Enigo, text: &str) -> Result<()> {
    clipboard.set_text(text)?;
    enigo.key_down(Key::Control);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Control);
    Ok(())
}

fn needs_clipboard_fallback(text: &str) -> bool {
    !text.is_ascii()
}
