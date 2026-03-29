//! Log-mel spectrogram extraction using rustfft.
//!
//! Produces an ndarray [n_mels × n_frames] f32 matrix from 16kHz mono PCM.
//! Parameters: 80 bins, 25ms Hann window, 10ms hop, 512-point FFT.
//! Compatible with Parakeet-TDT and Cohere Transcribe.

use ndarray::Array2;
use rustfft::{FftPlanner, num_complex::Complex};

pub const N_MELS: usize = 80;
const SAMPLE_RATE: usize = 16_000;
const HOP_LENGTH: usize = 160;     // 10ms at 16kHz
const WIN_LENGTH: usize = 400;     // 25ms at 16kHz
const N_FFT: usize = 512;

/// Convert raw 16kHz mono f32 samples to a log-mel spectrogram.
/// Returns shape [N_MELS, n_frames] in row-major order.
pub fn log_mel_spectrogram(samples: &[f32]) -> Array2<f32> {
    if samples.len() < WIN_LENGTH {
        return Array2::zeros((N_MELS, 1));
    }

    let n_frames = (samples.len() - WIN_LENGTH) / HOP_LENGTH + 1;
    let mut mel = Array2::<f32>::zeros((N_MELS, n_frames));

    // Hann window
    let window: Vec<f32> = (0..WIN_LENGTH)
        .map(|i| {
            0.5 * (1.0
                - (2.0 * std::f32::consts::PI * i as f32 / (WIN_LENGTH - 1) as f32).cos())
        })
        .collect();

    let filterbank = mel_filterbank();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);

    for (frame_idx, frame_start) in (0..=samples.len() - WIN_LENGTH)
        .step_by(HOP_LENGTH)
        .take(n_frames)
        .enumerate()
    {
        // Apply Hann window and zero-pad to N_FFT
        let mut buf: Vec<Complex<f32>> = (0..N_FFT)
            .map(|i| {
                let s = if i < WIN_LENGTH {
                    samples[frame_start + i] * window[i]
                } else {
                    0.0
                };
                Complex::new(s, 0.0)
            })
            .collect();

        fft.process(&mut buf);

        // Power spectrum: |X[k]|² for k in 0..=N_FFT/2
        let power: Vec<f32> = buf[..=N_FFT / 2]
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .collect();

        // Apply mel filterbank
        for (mel_bin, filter) in filterbank.iter().enumerate() {
            let energy: f32 = filter.iter().zip(&power).map(|(f, p)| f * p).sum();
            mel[[mel_bin, frame_idx]] = energy.max(1e-10_f32).ln();
        }
    }

    mel
}

/// Build 80-bin triangular mel filterbank. Returns Vec<Vec<f32>> of shape [N_MELS, N_FFT/2+1].
fn mel_filterbank() -> Vec<Vec<f32>> {
    let n_bins = N_FFT / 2 + 1;
    let fmin = 0.0_f32;
    let fmax = SAMPLE_RATE as f32 / 2.0;

    let hz_to_mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
    let mel_to_hz = |mel: f32| 700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0);

    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    let mel_points: Vec<f32> = (0..=N_MELS + 1)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f32 / (N_MELS + 1) as f32))
        .collect();

    let bin_freqs: Vec<f32> = (0..n_bins)
        .map(|i| i as f32 * SAMPLE_RATE as f32 / N_FFT as f32)
        .collect();

    (0..N_MELS)
        .map(|m| {
            bin_freqs
                .iter()
                .map(|&f| {
                    let lower = mel_points[m];
                    let center = mel_points[m + 1];
                    let upper = mel_points[m + 2];
                    if f >= lower && f <= center {
                        (f - lower) / (center - lower)
                    } else if f > center && f <= upper {
                        (upper - f) / (upper - center)
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect()
}
