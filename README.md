# WhisperWrite - Simple Speech Recognition with OpenAI Whisper API

### Installation
```bash
pip install -r requirements.txt
```

### OpenAI API key
- Create `.env` file in root directory
- Add `OPENAI_API_KEY = <your_api_key>`

### Commands
- `whisper-write` - Speech recognition
- `whisper-write -t` - Speech recognition with auto transcription to english

### How To Use
- Run `whisper-write` or `whisper-write -t`
- Focus on the window you want to type in
- Press `Right Ctrl` to start recording
- Release `Right Ctrl` to stop recording

### How To Use With Ueli
- Open ueli settings
- Go to shortcuts
- Add new shortcut
- Name: Speech Recognition
- Command: `whisper-write --admin` or `whisper-write --admin -t`
