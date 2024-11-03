import os
import openai

openai.api_key = os.getenv('OPENAI_API_KEY')

def transcript_audio(audio_file):
    return openai.Audio.transcribe("whisper-1", audio_file)["text"].strip()

def translate_audio(audio_file):
    return openai.Audio.translate("whisper-1", audio_file)["text"].strip()