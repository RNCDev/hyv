use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

const TARGET_SAMPLE_RATE: u32 = 16000;

// ─── Microphone Capture (CPAL) ───────────────────────────────────────────────

pub struct MicCapture {
    stream: Option<cpal::Stream>,
}

impl MicCapture {
    pub fn new() -> Self {
        Self { stream: None }
    }

    pub fn start(
        &mut self,
        buffer: Arc<Mutex<Vec<f32>>>,
        active: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No microphone found")?;

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get mic config: {e}"))?;

        let device_sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        info!(
            device = ?device.name().unwrap_or_default(),
            sample_rate = device_sample_rate,
            channels,
            "Microphone capture starting"
        );

        let stream = match config.sample_format() {
            SampleFormat::F32 => self.build_stream::<f32>(
                &device,
                &config.into(),
                buffer,
                active,
                device_sample_rate,
                channels,
            ),
            SampleFormat::I16 => self.build_stream::<i16>(
                &device,
                &config.into(),
                buffer,
                active,
                device_sample_rate,
                channels,
            ),
            format => Err(format!("Unsupported sample format: {format:?}")),
        }?;

        stream
            .play()
            .map_err(|e| format!("Failed to start mic stream: {e}"))?;
        self.stream = Some(stream);
        info!("Microphone capture started");
        Ok(())
    }

    fn build_stream<T: cpal::Sample + cpal::SizedSample + Send + 'static>(
        &self,
        device: &cpal::Device,
        config: &StreamConfig,
        buffer: Arc<Mutex<Vec<f32>>>,
        active: Arc<AtomicBool>,
        device_sample_rate: u32,
        channels: usize,
    ) -> Result<cpal::Stream, String>
    where
        f32: FromSample<T>,
    {
        let needs_resample = device_sample_rate != TARGET_SAMPLE_RATE;
        let ratio = if needs_resample {
            TARGET_SAMPLE_RATE as f64 / device_sample_rate as f64
        } else {
            1.0
        };

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if !active.load(Ordering::Relaxed) {
                        return;
                    }

                    // Convert to f32 and mix to mono
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| {
                            let sum: f32 = frame
                                .iter()
                                .map(|s| <f32 as FromSample<T>>::from_sample_(*s))
                                .sum();
                            sum / channels as f32
                        })
                        .collect();

                    let resampled = if needs_resample {
                        linear_resample(&mono, ratio)
                    } else {
                        mono
                    };

                    if let Ok(mut buf) = buffer.try_lock() {
                        buf.extend_from_slice(&resampled);
                    }
                },
                move |err| {
                    error!("Microphone stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build mic stream: {e}"))?;

        Ok(stream)
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            info!("Microphone capture stopped");
        }
    }
}

fn linear_resample(input: &[f32], ratio: f64) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    let output_len = (input.len() as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let sample = if idx + 1 < input.len() {
            input[idx] * (1.0 - frac as f32) + input[idx + 1] * frac as f32
        } else {
            input[idx.min(input.len() - 1)]
        };
        output.push(sample);
    }
    output
}

// ─── System Audio Capture (Core Audio Process Tap via cidre) ──────────────────

#[cfg(target_os = "macos")]
mod core_audio_tap {
    use cidre::{arc, av, cat, cf, core_audio as ca, ns, os};
    use ringbuf::{
        traits::{Consumer, Producer, Split},
        HeapCons, HeapProd, HeapRb,
    };
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    
    use tracing::info;

    use super::TARGET_SAMPLE_RATE;

    struct AudioContext {
        format: arc::R<av::AudioFormat>,
        producer: HeapProd<f32>,
        should_terminate: Arc<AtomicBool>,
        sample_rate: Arc<AtomicU32>,
        consecutive_drops: u32,
    }

    pub struct SystemAudioCapture {
        // Held to keep the tap alive
        _tap: ca::TapGuard,
        _started_device: ca::hardware::StartedDevice<ca::AggregateDevice>,
        _ctx: Box<AudioContext>,
        consumer: HeapCons<f32>,
        should_terminate: Arc<AtomicBool>,
        sample_rate: Arc<AtomicU32>,
    }

    impl SystemAudioCapture {
        pub fn new() -> Result<(Self, Arc<AtomicU32>), String> {
            // Get default output device
            let output_device = ca::System::default_output_device()
                .map_err(|e| format!("No output device: {e:?}"))?;
            let output_uid = output_device
                .uid()
                .map_err(|e| format!("No output UID: {e:?}"))?;

            info!(device = %output_uid, "Creating system audio tap");

            // Create mono global tap (captures all system audio)
            let tap_desc = ca::TapDesc::with_mono_global_tap_excluding_processes(&ns::Array::new());
            let tap = tap_desc
                .create_process_tap()
                .map_err(|e| format!("Failed to create process tap: {e:?}"))?;

            let tap_uid = tap
                .uid()
                .map_err(|e| format!("Failed to get tap UID: {e:?}"))?;

            // Build aggregate device with ONLY the tap (not the output device)
            // Including the output device causes duplicate/echo audio
            let sub_tap = cf::DictionaryOf::with_keys_values(
                &[ca::sub_device_keys::uid()],
                &[tap_uid.as_type_ref()],
            );

            let agg_desc = cf::DictionaryOf::with_keys_values(
                &[
                    ca::aggregate_device_keys::is_private(),
                    ca::aggregate_device_keys::is_stacked(),
                    ca::aggregate_device_keys::tap_auto_start(),
                    ca::aggregate_device_keys::name(),
                    ca::aggregate_device_keys::main_sub_device(),
                    ca::aggregate_device_keys::uid(),
                    ca::aggregate_device_keys::tap_list(),
                ],
                &[
                    cf::Boolean::value_true().as_type_ref(),
                    cf::Boolean::value_false(),
                    cf::Boolean::value_true(),
                    cf::str!(c"hyv-audio-tap"),
                    &output_uid,
                    &cf::Uuid::new().to_cf_string(),
                    &cf::ArrayOf::from_slice(&[sub_tap.as_ref()]),
                ],
            );

            let agg_device = ca::AggregateDevice::with_desc(&agg_desc)
                .map_err(|e| format!("Failed to create aggregate device: {e:?}"))?;

            // Get the tap's audio format
            let asbd = tap
                .asbd()
                .map_err(|e| format!("Failed to get tap format: {e:?}"))?;
            let format = av::AudioFormat::with_asbd(&asbd)
                .ok_or("Failed to create audio format from tap")?;

            let device_sample_rate = asbd.sample_rate as u32;
            info!(
                sample_rate = device_sample_rate,
                channels = asbd.channels_per_frame,
                "System audio tap format"
            );

            // Ring buffer for lock-free audio transfer
            let buffer_size = 1024 * 128; // ~8 seconds at 16kHz
            let rb = HeapRb::<f32>::new(buffer_size);
            let (producer, consumer) = rb.split();

            let should_terminate = Arc::new(AtomicBool::new(false));
            let sample_rate = Arc::new(AtomicU32::new(device_sample_rate));

            let mut ctx = Box::new(AudioContext {
                format,
                producer,
                should_terminate: should_terminate.clone(),
                sample_rate: sample_rate.clone(),
                consecutive_drops: 0,
            });

            // Audio IO callback
            extern "C" fn audio_proc(
                _device: ca::Device,
                _now: &cat::AudioTimeStamp,
                input_data: &cat::AudioBufList<1>,
                _input_time: &cat::AudioTimeStamp,
                _output_data: &mut cat::AudioBufList<1>,
                _output_time: &cat::AudioTimeStamp,
                ctx: Option<&mut AudioContext>,
            ) -> os::Status {
                let ctx = match ctx {
                    Some(c) => c,
                    None => return Default::default(),
                };

                if ctx.should_terminate.load(Ordering::Relaxed) {
                    return Default::default();
                }

                // Try to get PCM buffer view
                let buf = match av::AudioPcmBuf::with_buf_list_no_copy(
                    &ctx.format,
                    input_data,
                    None,
                ) {
                    Some(buf) => buf,
                    None => return Default::default(),
                };

                // Extract f32 samples from channel 0 (mono tap)
                if let Some(samples) = buf.data_f32_at(0) {
                    let pushed = ctx.producer.push_slice(samples);
                    if pushed < samples.len() {
                        ctx.consecutive_drops += 1;
                        if ctx.consecutive_drops > 10 {
                            ctx.should_terminate.store(true, Ordering::Relaxed);
                        }
                    } else {
                        ctx.consecutive_drops = 0;
                    }
                }

                Default::default()
            }

            // Start the device with our IO proc
            let proc_id = agg_device
                .create_io_proc_id(audio_proc, Some(&mut *ctx))
                .map_err(|e| format!("Failed to create IO proc: {e:?}"))?;

            let started_device = ca::device_start(agg_device, Some(proc_id))
                .map_err(|e| format!("Failed to start audio device: {e:?}"))?;

            info!("System audio capture started via Core Audio Process Tap");

            let sr = sample_rate.clone();
            Ok((
                Self {
                    _tap: tap,
                    _started_device: started_device,
                    _ctx: ctx,
                    consumer,
                    should_terminate,
                    sample_rate: sample_rate.clone(),
                },
                sr,
            ))
        }

        /// Drain all available samples from the ring buffer into the target buffer.
        /// Resamples to TARGET_SAMPLE_RATE if needed.
        pub fn drain_into(&mut self, target: &mut Vec<f32>) {
            let mut temp = vec![0.0f32; 4096];
            loop {
                let count = self.consumer.pop_slice(&mut temp);
                if count == 0 {
                    break;
                }

                let device_sr = self.sample_rate.load(Ordering::Relaxed);
                if device_sr != TARGET_SAMPLE_RATE && device_sr > 0 {
                    let ratio = TARGET_SAMPLE_RATE as f64 / device_sr as f64;
                    let resampled = super::linear_resample(&temp[..count], ratio);
                    target.extend_from_slice(&resampled);
                } else {
                    target.extend_from_slice(&temp[..count]);
                }
            }
        }

        pub fn is_terminated(&self) -> bool {
            self.should_terminate.load(Ordering::Relaxed)
        }
    }

    impl Drop for SystemAudioCapture {
        fn drop(&mut self) {
            self.should_terminate.store(true, Ordering::Relaxed);
            info!("System audio capture stopped");
        }
    }
}

// ─── Public SystemCapture wrapper ─────────────────────────────────────────────

pub struct SystemCapture {
    #[cfg(target_os = "macos")]
    inner: Option<core_audio_tap::SystemAudioCapture>,
}

impl SystemCapture {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: None,
        }
    }

    pub fn start(
        &mut self,
        buffer: Arc<Mutex<Vec<f32>>>,
        active: Arc<AtomicBool>,
    ) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let (mut capture, _sample_rate) = core_audio_tap::SystemAudioCapture::new()?;

            // Spawn a thread that periodically drains the ring buffer into the shared buffer
            let active_clone = active.clone();
            let buffer_clone = buffer.clone();

            std::thread::spawn(move || {
                while active_clone.load(Ordering::Relaxed) && !capture.is_terminated() {
                    let mut local_buf = Vec::new();
                    capture.drain_into(&mut local_buf);

                    if !local_buf.is_empty() {
                        if let Ok(mut buf) = buffer_clone.try_lock() {
                            buf.extend_from_slice(&local_buf);
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                info!("System audio drain thread exiting");
            });

            // We don't store `inner` since the capture is moved into the thread.
            // The thread owns it and drops it when recording stops.
        }

        #[cfg(not(target_os = "macos"))]
        {
            warn!("System audio capture not available on this platform");
        }

        Ok(())
    }

    pub fn stop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.inner = None;
        }
        info!("System audio capture stopped");
    }
}
