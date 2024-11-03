from time import sleep
import numpy as np
import pyaudio
import wave
import platform
import threading
from pynput import keyboard

if platform.system() == 'Windows':
    import winsound

FORMAT = pyaudio.paInt16
CHANNELS = 1
RATE = 22050
CHUNK_SIZE = 1024
FRAMES_PER_SEC = RATE / CHUNK_SIZE
CANCEL_RECORDING_TIME = 0.1

is_right_ctrl_pressed = False

def on_press(key):
    if key == keyboard.Key.ctrl_r:
        global is_right_ctrl_pressed
        is_right_ctrl_pressed = True
        return True

def on_release(key):
    if key == keyboard.Key.ctrl_r:
        global is_right_ctrl_pressed
        is_right_ctrl_pressed = False
        return True
    
def start_listen_right_ctrl():
    with keyboard.Listener(on_press=on_press, on_release=on_release) as listener:
        listener.join()

threading.Thread(target=start_listen_right_ctrl).start()

def listen_and_save(filename):
    print("Press 'Right Shift' to start recording")
    while not is_right_ctrl_pressed:
        sleep(0.1)
    print("Recording...")

    audio = pyaudio.PyAudio()
    stream = audio.open(format=FORMAT, channels=CHANNELS,
                        rate=RATE, input=True,
                        frames_per_buffer=CHUNK_SIZE)
    audio_frames = []
    
    if platform.system() == 'Windows':
        def play_beep():
            winsound.Beep(1000, 300)
        threading.Thread(target=play_beep).start()

    try:
        while True:
            audio_data = np.frombuffer(stream.read(CHUNK_SIZE), dtype=np.int16)
            audio_frames.append(audio_data)
            if not is_right_ctrl_pressed:
                break

    except KeyboardInterrupt:
        return False
    finally:
        stream.stop_stream()
        stream.close()
        audio.terminate()
    
    if len(audio_frames) < FRAMES_PER_SEC * CANCEL_RECORDING_TIME:
        print("No audio recorded")
        return False

    with wave.open(filename, 'wb') as wf:
        wf.setnchannels(CHANNELS)
        wf.setsampwidth(audio.get_sample_size(FORMAT))
        wf.setframerate(RATE)
        wf.writeframes(b''.join(audio_frames))

    print("Recording finished")
    return True