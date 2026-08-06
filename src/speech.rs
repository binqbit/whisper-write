use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use crossbeam_channel::{RecvTimeoutError, SendTimeoutError, Sender};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SpeechConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_size: usize,
    pub min_speech: Duration,
    pub max_trailing_silence: Duration,
    pub start_trigger_chunks: usize,
    pub energy_history: Duration,
    pub energy_threshold_multiplier: f32,
    pub absolute_min_energy: f32,
    pub continuation_ratio: f32,
    pub preroll: Duration,
}

impl SpeechConfig {
    pub fn validate(&self) -> Result<()> {
        if self.sample_rate == 0 {
            return Err(anyhow!("sample_rate must be > 0"));
        }
        if self.channels == 0 {
            return Err(anyhow!("channels must be > 0"));
        }
        if self.chunk_size == 0 {
            return Err(anyhow!("chunk_size must be > 0"));
        }
        if self.energy_threshold_multiplier <= 0.0 {
            return Err(anyhow!("energy_threshold_multiplier must be > 0"));
        }
        if self.absolute_min_energy < 0.0 {
            return Err(anyhow!("absolute_min_energy must be >= 0"));
        }
        if !(0.0..=1.0).contains(&self.continuation_ratio) {
            return Err(anyhow!("continuation_ratio must be between 0 and 1"));
        }
        Ok(())
    }
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            sample_rate: 22_050,
            channels: 1,
            chunk_size: 256,
            min_speech: Duration::from_millis(700),
            max_trailing_silence: Duration::from_millis(900),
            start_trigger_chunks: 3,
            energy_history: Duration::from_secs_f32(3.0),
            energy_threshold_multiplier: 2.6,
            absolute_min_energy: 200.0,
            continuation_ratio: 0.35,
            preroll: Duration::from_millis(500),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpeechSegment {
    pub audio_bytes: Vec<u8>,
    pub duration: Duration,
    pub sequence_id: u64,
}

pub struct SpeechListener {
    config: SpeechConfig,
    device_index: Option<usize>,
}

impl SpeechListener {
    pub fn new(config: SpeechConfig, device_index: Option<usize>) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            device_index,
        })
    }

    pub fn listen_once(&self) -> Result<SpeechSegment> {
        let (segment_tx, segment_rx) = crossbeam_channel::bounded(4);
        let mut listener =
            ContinuousSpeechListener::new(self.config.clone(), self.device_index, segment_tx);
        listener.start();

        let segment = match segment_rx.recv() {
            Ok(segment) => segment,
            Err(err) => {
                listener.stop();
                return Err(anyhow!("Speech listener stopped: {err}"));
            }
        };

        listener.stop();
        Ok(segment)
    }
}

struct ContinuousSpeechListener {
    config: SpeechConfig,
    output_sender: Option<Sender<SpeechSegment>>,
    device_index: Option<usize>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ContinuousSpeechListener {
    fn new(
        config: SpeechConfig,
        device_index: Option<usize>,
        output_sender: Sender<SpeechSegment>,
    ) -> Self {
        Self {
            config,
            output_sender: Some(output_sender),
            device_index,
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        let Some(output_sender) = self.output_sender.take() else {
            return;
        };
        let stop_flag = Arc::clone(&self.stop_flag);
        let config = self.config.clone();
        let device_index = self.device_index;
        self.handle = Some(thread::spawn(move || {
            if let Err(err) = run_listener(config, output_sender, stop_flag, device_index) {
                eprintln!("Speech listener error: {err}");
            }
        }));
    }

    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_listener(
    config: SpeechConfig,
    output_sender: Sender<SpeechSegment>,
    stop_flag: Arc<AtomicBool>,
    device_index: Option<usize>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = select_device(&host, device_index).context("No input audio device available")?;

    let default_config = device
        .default_input_config()
        .context("Unable to query default input config")?;
    let sample_format = default_config.sample_format();
    let default_stream_config: cpal::StreamConfig = default_config.into();

    let mut preferred_config = default_stream_config.clone();
    preferred_config.channels = config.channels;
    preferred_config.sample_rate = cpal::SampleRate(config.sample_rate);

    let (chunk_tx, chunk_rx) = crossbeam_channel::bounded(32);

    let (stream, stream_config) = match build_input_stream(
        &device,
        &preferred_config,
        sample_format,
        config.chunk_size,
        chunk_tx.clone(),
    ) {
        Ok(stream) => (stream, preferred_config),
        Err(err) => {
            eprintln!(
                "Unable to open requested audio stream ({err}). Falling back to default device config."
            );
            let stream = build_input_stream(
                &device,
                &default_stream_config,
                sample_format,
                config.chunk_size,
                chunk_tx.clone(),
            )?;
            (stream, default_stream_config)
        }
    };

    stream.play().context("Unable to start audio stream")?;
    drop(chunk_tx);

    let actual_rate = stream_config.sample_rate.0;
    let mut detector = SpeechDetector::new(config, actual_rate);

    while !stop_flag.load(Ordering::SeqCst) {
        let chunk = match chunk_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => chunk,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        match detector.process_chunk(chunk) {
            SpeechEvent::None => {}
            SpeechEvent::Detected => println!("Speech detected."),
            SpeechEvent::Discarded => println!("Discarded short noise segment."),
            SpeechEvent::Segment(segment) => {
                let mut pending = Some(segment);
                while let Some(segment) = pending {
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let sequence_id = segment.sequence_id;
                    let duration = segment.duration;
                    match output_sender.send_timeout(segment, Duration::from_millis(500)) {
                        Ok(()) => {
                            println!(
                                "Segment #{} captured ({:.1}s).",
                                sequence_id,
                                duration.as_secs_f32()
                            );
                            pending = None;
                        }
                        Err(SendTimeoutError::Timeout(segment)) => pending = Some(segment),
                        Err(SendTimeoutError::Disconnected(_)) => pending = None,
                    }
                }
            }
        }
    }

    Ok(())
}

fn select_device(host: &cpal::Host, device_index: Option<usize>) -> Option<cpal::Device> {
    if let Some(index) = device_index {
        if let Ok(mut devices) = host.input_devices() {
            return devices.nth(index);
        }
    }
    host.default_input_device()
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    chunk_size: usize,
    chunk_tx: Sender<Vec<i16>>,
) -> Result<cpal::Stream> {
    let stream = match sample_format {
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(device, config, chunk_tx, chunk_size, |err| {
                eprintln!("Audio stream error: {err}");
            })
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(device, config, chunk_tx, chunk_size, |err| {
                eprintln!("Audio stream error: {err}");
            })
        }
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(device, config, chunk_tx, chunk_size, |err| {
                eprintln!("Audio stream error: {err}");
            })
        }
        _ => Err(anyhow!("Unsupported sample format")),
    }?;

    Ok(stream)
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    chunk_tx: Sender<Vec<i16>>,
    chunk_size: usize,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    let mut leftover: Vec<i16> = Vec::new();

    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                handle_input_data(data, channels, chunk_size, &chunk_tx, &mut leftover)
            },
            err_fn,
            None,
        )
        .context("Failed to build input stream")
}

fn handle_input_data<T>(
    data: &[T],
    channels: usize,
    chunk_size: usize,
    chunk_tx: &Sender<Vec<i16>>,
    leftover: &mut Vec<i16>,
) where
    T: Sample,
    f32: cpal::FromSample<T>,
{
    if channels == 0 || chunk_size == 0 {
        return;
    }

    for frame in data.chunks(channels) {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for sample in frame {
            sum += (*sample).to_sample::<f32>();
            count += 1;
        }
        let mono = if count > 0 { sum / count as f32 } else { 0.0 };
        let scaled = (mono.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        leftover.push(scaled);

        if leftover.len() >= chunk_size {
            let chunk: Vec<i16> = leftover.drain(..chunk_size).collect();
            let _ = chunk_tx.try_send(chunk);
        }
    }
}

#[derive(Debug)]
enum SpeechEvent {
    None,
    Detected,
    Discarded,
    Segment(SpeechSegment),
}

struct SpeechDetector {
    config: SpeechConfig,
    rate: u32,
    energy_history: VecDeque<f32>,
    energy_history_size: usize,
    pre_roll_frames: VecDeque<Vec<i16>>,
    pre_roll_max: usize,
    speech_frames: Vec<Vec<i16>>,
    speech_active: bool,
    start_trigger_count: usize,
    trailing_silence_chunks: usize,
    max_trailing_silence_chunks: usize,
    min_required_samples: usize,
    sequence_id: u64,
}

impl SpeechDetector {
    fn new(config: SpeechConfig, actual_rate: u32) -> Self {
        let chunks_per_second = actual_rate as f32 / config.chunk_size as f32;
        let energy_history_size = (config.energy_history.as_secs_f32() * chunks_per_second)
            .floor()
            .max(1.0) as usize;
        let pre_roll_max = (config.preroll.as_secs_f32() * chunks_per_second)
            .floor()
            .max(1.0) as usize;
        let max_trailing_silence_chunks = (config.max_trailing_silence.as_secs_f32()
            * chunks_per_second)
            .floor()
            .max(1.0) as usize;
        let min_required_samples =
            (actual_rate as f32 * config.min_speech.as_secs_f32()).floor() as usize;

        let mut detector = Self {
            config,
            rate: actual_rate,
            energy_history: VecDeque::with_capacity(energy_history_size),
            energy_history_size,
            pre_roll_frames: VecDeque::with_capacity(pre_roll_max),
            pre_roll_max,
            speech_frames: Vec::new(),
            speech_active: false,
            start_trigger_count: 0,
            trailing_silence_chunks: 0,
            max_trailing_silence_chunks,
            min_required_samples,
            sequence_id: 0,
        };

        detector.seed_energy_history();
        detector
    }

    fn process_chunk(&mut self, chunk: Vec<i16>) -> SpeechEvent {
        if chunk.is_empty() {
            return SpeechEvent::None;
        }

        let energy = calculate_energy(&chunk);
        let threshold = determine_threshold(
            &self.energy_history,
            self.config.energy_threshold_multiplier,
            self.config.absolute_min_energy,
        );
        let continuation_threshold = threshold * self.config.continuation_ratio;

        if !self.speech_active {
            if energy >= threshold {
                self.start_trigger_count = self.start_trigger_count.saturating_add(1);
                if self.start_trigger_count >= self.config.start_trigger_chunks {
                    self.speech_active = true;
                    self.speech_frames = vec![chunk];
                    self.trailing_silence_chunks = 0;
                    self.start_trigger_count = 0;
                    return SpeechEvent::Detected;
                }
                self.push_preroll(chunk);
                self.push_energy(energy);
                return SpeechEvent::None;
            }
            self.start_trigger_count = 0;
            self.push_preroll(chunk);
            self.push_energy(energy);
            return SpeechEvent::None;
        }

        self.speech_frames.push(chunk);
        self.trailing_silence_chunks += 1;

        if energy >= continuation_threshold {
            self.trailing_silence_chunks = 0;
            return SpeechEvent::None;
        }

        if self.trailing_silence_chunks < self.max_trailing_silence_chunks {
            return SpeechEvent::None;
        }

        // Allow speech to "break" a silence window if it comes right at the edge.
        if energy >= threshold {
            self.trailing_silence_chunks = 0;
            return SpeechEvent::None;
        }

        self.finish_segment()
    }

    fn push_preroll(&mut self, chunk: Vec<i16>) {
        if self.pre_roll_frames.len() == self.pre_roll_max {
            self.pre_roll_frames.pop_front();
        }
        self.pre_roll_frames.push_back(chunk);
    }

    fn push_energy(&mut self, energy: f32) {
        if self.energy_history.len() == self.energy_history_size {
            self.energy_history.pop_front();
        }
        self.energy_history.push_back(energy);
    }

    fn finish_segment(&mut self) -> SpeechEvent {
        self.speech_active = false;

        let mut pre_roll_samples = 0usize;
        for frame in &self.pre_roll_frames {
            pre_roll_samples += frame.len();
        }

        let mut samples: Vec<i16> = Vec::new();
        for frame in &self.pre_roll_frames {
            samples.extend_from_slice(frame);
        }
        for frame in &self.speech_frames {
            samples.extend_from_slice(frame);
        }

        let total_samples = samples.len();
        let speech_samples = total_samples.saturating_sub(pre_roll_samples);

        let event = if speech_samples >= self.min_required_samples {
            let duration = Duration::from_secs_f32(speech_samples as f32 / self.rate as f32);
            match encode_wave(&samples, self.rate) {
                Ok(audio_bytes) => {
                    self.sequence_id += 1;
                    SpeechEvent::Segment(SpeechSegment {
                        audio_bytes,
                        duration,
                        sequence_id: self.sequence_id,
                    })
                }
                Err(err) => {
                    eprintln!("Failed to encode WAV: {err}");
                    SpeechEvent::Discarded
                }
            }
        } else {
            SpeechEvent::Discarded
        };

        self.speech_frames.clear();
        self.trailing_silence_chunks = 0;
        self.pre_roll_frames.clear();
        self.start_trigger_count = 0;
        self.clear_energy_history();

        event
    }

    fn seed_energy_history(&mut self) {
        self.energy_history.clear();
        for _ in 0..self.energy_history_size {
            self.energy_history
                .push_back(self.config.absolute_min_energy);
        }
    }

    fn clear_energy_history(&mut self) {
        self.energy_history.clear();
    }
}

fn calculate_energy(chunk: &[i16]) -> f32 {
    if chunk.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for sample in chunk {
        sum += (*sample as f32).abs();
    }
    let mean = sum / chunk.len() as f32;
    if mean.is_nan() {
        0.0
    } else {
        mean
    }
}

fn determine_threshold(history: &VecDeque<f32>, multiplier: f32, absolute_min: f32) -> f32 {
    if history.is_empty() {
        return absolute_min;
    }
    let mut values: Vec<f32> = history.iter().copied().collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    let median = if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
    .max(0.0);
    let dynamic_threshold = median * multiplier;
    if dynamic_threshold > absolute_min {
        dynamic_threshold
    } else {
        absolute_min
    }
}

fn encode_wave(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut cursor, spec)?;
        for sample in samples {
            writer.write_sample(*sample)?;
        }
        writer.finalize()?;
    }

    Ok(cursor.into_inner())
}
