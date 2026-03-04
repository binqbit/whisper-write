use anyhow::Result;
use clap::{Parser, ValueEnum};

use crate::openai_client::OpenAiConfig;
use crate::speech::SpeechConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Transcribe,
    Translate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputMode {
    Auto,
    Type,
    Paste,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub mode: Mode,
    pub output_mode: OutputMode,
    pub device_index: Option<usize>,
    pub speech: SpeechConfig,
    pub openai: OpenAiConfig,
    pub daemon: bool,
    pub continuous: bool,
    pub restore_clipboard: bool,
}

impl AppConfig {
    pub fn from_env_and_args() -> Result<Self> {
        let args = Args::parse();
        let openai = OpenAiConfig::from_env()?;
        let speech = SpeechConfig::default();
        speech.validate()?;

        Ok(Self {
            mode: if args.translate {
                Mode::Translate
            } else {
                Mode::Transcribe
            },
            output_mode: args.output,
            device_index: args.device_index,
            speech,
            openai,
            daemon: args.daemon,
            continuous: args.continuous,
            restore_clipboard: args.restore_clipboard,
        })
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "whisper-write",
    version,
    about = "Speech recognition that types or pastes into the active input."
)]
struct Args {
    /// Translate speech to English (uses /audio/translations)
    #[arg(short = 't', long = "translate")]
    translate: bool,

    /// Output mode: type directly or paste via clipboard
    #[arg(long = "output", value_enum, default_value_t = OutputMode::Auto)]
    output: OutputMode,

    /// Select input device index from the default host enumeration
    #[arg(long = "device", value_parser = clap::value_parser!(usize))]
    device_index: Option<usize>,

    /// Run as a background daemon (Linux only)
    #[arg(long = "daemon")]
    daemon: bool,

    /// Keep listening after each segment instead of exiting after one
    #[arg(short = 'c', long = "continuous")]
    continuous: bool,

    /// Restore previous clipboard contents after paste
    #[arg(long = "restore-clipboard")]
    restore_clipboard: bool,
}
