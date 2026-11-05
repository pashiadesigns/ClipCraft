//! Five seconds of measured effect history, independent of UI refresh rate.
pub const BINS: usize = 250;
const FRAMES: usize = 960;
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub input: f32,
    pub output: f32,
    pub input_rms: f32,
    pub output_rms: f32,
    pub change: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct History {
    pub points: [Point; BINS],
    pub len: usize,
}
impl Default for History {
    fn default() -> Self {
        Self {
            points: [Point::default(); BINS],
            len: 0,
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct LiveHistory {
    value: History,
    cursor: usize,
    frames: usize,
    peak: [f64; 2],
    energy: [f64; 2],
}
impl LiveHistory {
    pub fn push(&mut self, input: [f64; 2], output: [f64; 2]) {
        for (i, pair) in [input, output].into_iter().enumerate() {
            let pair = pair.map(|s| {
                if s.is_finite() {
                    s.clamp(-64., 64.)
                } else {
                    0.
                }
            });
            self.peak[i] = self.peak[i].max(pair[0].abs().max(pair[1].abs()));
            self.energy[i] += (pair[0] * pair[0] + pair[1] * pair[1]) * 0.5;
        }
        self.frames += 1;
        if self.frames == FRAMES {
            let rms = self.energy.map(|v| (v / FRAMES as f64).sqrt());
            self.value.points[self.cursor] = Point {
                input: self.peak[0] as f32,
                output: self.peak[1] as f32,
                input_rms: rms[0] as f32,
                output_rms: rms[1] as f32,
                change: if rms[0] > 1e-7 {
                    (super::db(rms[1]) - super::db(rms[0])).clamp(-48., 48.) as f32
                } else {
                    0.
                },
            };
            self.cursor = (self.cursor + 1) % BINS;
            self.value.len = (self.value.len + 1).min(BINS);
            self.frames = 0;
            self.peak = [0.; 2];
            self.energy = [0.; 2];
        }
    }
    pub fn snapshot(&self) -> History {
        let mut out = History {
            len: self.value.len,
            ..Default::default()
        };
        let start = if self.value.len == BINS {
            self.cursor
        } else {
            0
        };
        for i in 0..self.value.len {
            out.points[i] = self.value.points[(start + i) % BINS];
        }
        out
    }
}
