//! Playback-only, bounded stereo spectrum. Hann-windowed FFT; no audio mutation.
use super::{SAMPLE_RATE, db};
const N: usize = 4096;
pub const BINS: usize = 128;
#[derive(Clone, Copy, Debug)]
pub struct Spectrum {
    pub input: [f32; BINS],
    pub output: [f32; BINS],
}
impl Default for Spectrum {
    fn default() -> Self {
        Self {
            input: [-90.; BINS],
            output: [-90.; BINS],
        }
    }
}
impl Spectrum {
    pub fn frequency(index: usize) -> f64 {
        20. * 1000f64.powf(index as f64 / (BINS - 1) as f64)
    }
}
#[derive(Clone)]
pub(super) struct LiveSpectrum {
    ring: Vec<[f64; 4]>,
    cursor: usize,
    count: usize,
    pub value: Spectrum,
}
impl Default for LiveSpectrum {
    fn default() -> Self {
        Self {
            ring: vec![[0.; 4]; N],
            cursor: 0,
            count: 0,
            value: Spectrum::default(),
        }
    }
}
impl LiveSpectrum {
    pub fn push(&mut self, input: [f64; 2], output: [f64; 2]) {
        self.ring[self.cursor] = [input[0], input[1], output[0], output[1]];
        self.cursor = (self.cursor + 1) % N;
        self.count += 1;
        if self.count >= N && self.count.is_multiple_of(N / 2) {
            self.analyze();
        }
    }
    fn analyze(&mut self) {
        let mut powers = vec![[0.; 2]; N / 2 + 1];
        for channel in 0..4 {
            let mut data: Vec<[f64; 2]> = (0..N)
                .map(|i| {
                    let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / N as f64).cos();
                    [self.ring[(self.cursor + i) % N][channel] * window, 0.]
                })
                .collect();
            fft(&mut data);
            for (p, c) in powers.iter_mut().zip(data) {
                p[channel / 2] += (c[0] * c[0] + c[1] * c[1]) * 0.5;
            }
        }
        for i in 0..BINS {
            let f = Spectrum::frequency(i);
            let step = 1000f64.powf(0.5 / (BINS - 1) as f64);
            let lo = ((f / step * N as f64 / SAMPLE_RATE).floor() as usize).clamp(1, N / 2);
            let hi = ((f * step * N as f64 / SAMPLE_RATE).ceil() as usize).clamp(lo + 1, N / 2 + 1);
            for channel in 0..2 {
                let energy = powers[lo..hi].iter().map(|p| p[channel]).fold(0., f64::max);
                let value = (db(energy.sqrt() * 4. / N as f64) as f32).clamp(-90., 6.);
                let prior = if channel == 0 {
                    &mut self.value.input[i]
                } else {
                    &mut self.value.output[i]
                };
                *prior = if value > *prior {
                    value
                } else {
                    *prior * 0.75 + value * 0.25
                };
            }
        }
    }
}
fn fft(data: &mut [[f64; 2]]) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let angle = -std::f64::consts::TAU / length as f64;
        let root = [angle.cos(), angle.sin()];
        for base in (0..n).step_by(length) {
            let mut w = [1., 0.];
            for j in 0..length / 2 {
                let a = data[base + j];
                let b = data[base + j + length / 2];
                let v = [b[0] * w[0] - b[1] * w[1], b[0] * w[1] + b[1] * w[0]];
                data[base + j] = [a[0] + v[0], a[1] + v[1]];
                data[base + j + length / 2] = [a[0] - v[0], a[1] - v[1]];
                w = [
                    w[0] * root[0] - w[1] * root[1],
                    w[0] * root[1] + w[1] * root[0],
                ];
            }
        }
        length *= 2;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spectrum_locates_tones_without_stereo_phase_cancellation() {
        let mut s = LiveSpectrum::default();
        let f = 1500.;
        for i in 0..N {
            let x = (std::f64::consts::TAU * f * i as f64 / SAMPLE_RATE).sin() * 0.5;
            s.push([x, -x], [x * 0.5, -x * 0.5]);
        }
        let index = s
            .value
            .input
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!((Spectrum::frequency(index) / f - 1.).abs() < 0.08);
        assert!((s.value.input[index] + 6.02).abs() < 0.3);
        assert!((s.value.input[index] - s.value.output[index] - 6.02).abs() < 0.1);
    }
    #[test]
    fn spectrum_silence_is_finite_and_bounded() {
        let mut s = LiveSpectrum::default();
        for _ in 0..N * 3 {
            s.push([0.; 2], [0.; 2]);
        }
        assert!(
            s.value
                .input
                .iter()
                .chain(&s.value.output)
                .all(|v| *v == -90.)
        );
        assert_eq!(s.ring.len(), N);
    }
}
