import os
import keyboard
import dotenv
dotenv.load_dotenv()

from utils.whisper import transcript_audio, translate_audio
from utils.speech import listen_and_save

TEMP_FILE = "./data/voice.wav"

def main():
    is_translate = "-t" in os.sys.argv
    while True:
        if listen_and_save(TEMP_FILE):
            audio_file = open(TEMP_FILE, "rb")
            print("Transcribing audio...")
            if is_translate:
                text = translate_audio(audio_file)
            else:
                text = transcript_audio(audio_file)
            print("Text: ", text)
            keyboard.write(text + " ")
        print()

if __name__ == "__main__":
    main()
