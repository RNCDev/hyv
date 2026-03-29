//! Log-mel spectrogram extraction using rustfft.
//!
//! Produces an ndarray [n_mels × n_frames] f32 matrix from 16kHz mono PCM.
//! Parameters: 80 bins, 25ms Hann window, 10ms hop, 512-point FFT.
//! Compatible with Parakeet-TDT and Cohere Transcribe.

use ndarray::Array2;
use rustfft::{FftPlanner, num_complex::Complex};

pub const N_MELS: usize = 80;
pub const COHERE_N_MELS: usize = 128;
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

    let filterbank = mel_filterbank(N_MELS);

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

/// Build n_mels-bin triangular mel filterbank. Returns Vec<Vec<f32>> of shape [n_mels, N_FFT/2+1].
fn mel_filterbank(n_mels: usize) -> Vec<Vec<f32>> {
    let n_bins = N_FFT / 2 + 1;
    let fmin = 0.0_f32;
    let fmax = SAMPLE_RATE as f32 / 2.0;

    let hz_to_mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
    let mel_to_hz = |mel: f32| 700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0);

    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32))
        .collect();

    let bin_freqs: Vec<f32> = (0..n_bins)
        .map(|i| i as f32 * SAMPLE_RATE as f32 / N_FFT as f32)
        .collect();

    (0..n_mels)
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

/// Cohere-compatible log-mel spectrogram.
/// Returns [n_frames, 128] (time-major — matches encoder input `[batch, sequence_length, 128]`).
/// Differences from log_mel_spectrogram: 128 bins, preemphasis, per-feature normalization, transposed.
pub fn cohere_mel_spectrogram(samples: &[f32]) -> ndarray::Array2<f32> {
    use ndarray::{Array2, Axis};

    if samples.len() < WIN_LENGTH {
        return Array2::zeros((1, COHERE_N_MELS));
    }

    // 1. Preemphasis: s[i] = s[i] - 0.97 * s[i-1]
    let mut pre = samples.to_vec();
    for i in (1..pre.len()).rev() {
        pre[i] -= 0.97 * pre[i - 1];
    }

    let n_frames = (pre.len() - WIN_LENGTH) / HOP_LENGTH + 1;
    // Build [COHERE_N_MELS, n_frames] first, then transpose
    let mut mel = Array2::<f32>::zeros((COHERE_N_MELS, n_frames));

    // Hann window
    let window: Vec<f32> = (0..WIN_LENGTH)
        .map(|i| {
            0.5 * (1.0
                - (2.0 * std::f32::consts::PI * i as f32 / (WIN_LENGTH - 1) as f32).cos())
        })
        .collect();

    let filterbank = mel_filterbank(COHERE_N_MELS);

    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);

    for (frame_idx, frame_start) in (0..=pre.len() - WIN_LENGTH)
        .step_by(HOP_LENGTH)
        .take(n_frames)
        .enumerate()
    {
        let mut buf: Vec<rustfft::num_complex::Complex<f32>> = (0..N_FFT)
            .map(|i| {
                let s = if i < WIN_LENGTH { pre[frame_start + i] * window[i] } else { 0.0 };
                rustfft::num_complex::Complex::new(s, 0.0)
            })
            .collect();
        fft.process(&mut buf);
        let power: Vec<f32> = buf[..=N_FFT / 2]
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .collect();
        for (mel_bin, filter) in filterbank.iter().enumerate() {
            let energy: f32 = filter.iter().zip(&power).map(|(f, p)| f * p).sum();
            mel[[mel_bin, frame_idx]] = energy.max(1e-10_f32).ln();
        }
    }

    // 2. Per-feature normalization: for each mel row, subtract mean, divide by std+1e-5
    for mut row in mel.axis_iter_mut(Axis(0)) {
        let mean = row.mean().unwrap_or(0.0);
        let variance = row.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / row.len() as f32;
        let std = variance.sqrt();
        row.mapv_inplace(|x| (x - mean) / (std + 1e-5));
    }

    // 3. Transpose to [n_frames, COHERE_N_MELS]
    mel.t().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohere_mel_output_shape() {
        // 1 second of silence at 16kHz
        let samples = vec![0.0f32; 16_000];
        let mel = cohere_mel_spectrogram(&samples);
        // n_frames = (16000 - 400) / 160 + 1 = 98
        assert_eq!(mel.shape(), &[98, 128], "expected [n_frames, 128], got {:?}", mel.shape());
    }

    #[test]
    fn cohere_mel_preemphasis_changes_values() {
        let samples = vec![0.5f32; 16_000];
        let mel_cohere = cohere_mel_spectrogram(&samples);
        let mel_whisper = log_mel_spectrogram(&samples);
        assert_ne!(
            mel_cohere.sum(),
            mel_whisper.sum(),
            "cohere mel must differ from whisper mel (different bins + preemphasis)"
        );
    }

    #[test]
    fn existing_whisper_mel_unchanged() {
        let samples = vec![0.1f32; 16_000];
        let mel = log_mel_spectrogram(&samples);
        assert_eq!(mel.shape()[0], N_MELS, "whisper mel bins must stay 80");
        assert_eq!(mel.shape()[0], 80);
    }
}
