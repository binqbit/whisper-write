mod clipboard;
mod config;
mod daemon;
mod openai_client;
mod speech;
mod typing;

use anyhow::Result;

use crate::config::{AppConfig, Mode};
use crate::daemon::maybe_daemonize;
use crate::openai_client::{AudioFile, OpenAiClient};
use crate::speech::SpeechListener;
use crate::typing::Typer;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env_and_args()?;
    maybe_daemonize(config.daemon)?;

    let client = OpenAiClient::new(config.openai.clone())?;
    let listener = SpeechListener::new(config.speech.clone(), config.device_index)?;
    let typer = Typer::new(config.output_mode, config.restore_clipboard)?;

    let mut app = App {
        config,
        client,
        listener,
        typer,
    };

    app.run()
}

struct App {
    config: AppConfig,
    client: OpenAiClient,
    listener: SpeechListener,
    typer: Typer,
}

impl App {
    fn run(&mut self) -> Result<()> {
        if self.config.continuous {
            loop {
                self.run_once()?;
            }
        } else {
            self.run_once()
        }
    }

    fn run_once(&mut self) -> Result<()> {
        println!("Listening for speech... (Ctrl+C to exit)");
        let segment = self.listener.listen_once()?;
        println!("Transcribing audio...");

        let file = AudioFile::new(
            format!("segment-{}.wav", segment.sequence_id),
            segment.audio_bytes,
        );

        let text = match self.config.mode {
            Mode::Transcribe => self.client.transcribe_audio(file)?,
            Mode::Translate => self.client.translate_audio(file)?,
        };

        println!("Text: {}", text);
        if !text.is_empty() {
            let mut output = String::with_capacity(text.len() + 1);
            output.push_str(&text);
            output.push(' ');
            self.typer.type_text(&output)?;
        }
        println!();
        Ok(())
    }
}
