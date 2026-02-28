# WhisperWrite

WhisperWrite listens for speech, detects segments automatically, sends audio to the OpenAI Audio API, and types the result into the active window.

## Requirements
- Toolchain for building from source
- OpenAI API key

## Setup
Create a `.env` file in this folder and add:
```
OPENAI_API_KEY=your_api_key_here
```

## Build
```
cargo build --release
```

## Run
Transcribe speech:
```
cargo run --release
```

Translate to English:
```
cargo run --release -- -t
```

Optional flags:
- `--output <auto|type|paste>` chooses output method (default: auto).
- `--device <index>` selects a specific input device by index (from the default host enumeration).
- `--daemon` runs in the background (Linux only).
- `--continuous` keeps listening after each segment instead of exiting.

## Notes
- Translation uses the `/audio/translations` endpoint, which currently requires the `whisper-1` model.
- If you want to change models or the base URL, set:
  - `WHISPER_WRITE_TRANSCRIBE_MODEL`
  - `WHISPER_WRITE_TRANSLATE_MODEL`
  - `OPENAI_BASE_URL`
- Default output is `auto`: on Wayland it tries `wtype` (virtual keyboard); otherwise it falls back to clipboard paste.
- Use `--output type` to force direct typing, or `--output paste` to force clipboard paste.
- In Linux, when `--output type` receives non-ASCII text (for example Cyrillic), the app falls back to a clipboard paste path to preserve correct characters.
- The app does not depend on keyboard layout detection. By default it uses `auto` output:
  - On Wayland it tries `wtype` (virtual keyboard).
  - Otherwise it falls back to clipboard paste.

## Linux system dependencies
`cpal` uses ALSA on Linux. If you see build errors about `alsa`, install your distro's ALSA dev package (for example, `libasound2-dev` on Debian/Ubuntu).
