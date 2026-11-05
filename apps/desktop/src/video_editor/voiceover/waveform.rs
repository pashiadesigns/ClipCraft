//! Bounded peak overview; every recorded sample contributes, even for long takes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub peaks: Vec<[f32; 2]>,
    pub frames: u64,
    pub bucket: u64,
}
#[derive(Default)]
pub(crate) struct Overview {
    peaks: Vec<[f32; 2]>,
    partial: [f32; 2],
    count: u64,
    frames: u64,
    bucket: u64,
}
impl Overview {
    pub fn push(&mut self, samples: &[i16]) {
        if self.bucket == 0 {
            self.bucket = 2400;
        }
        for pair in samples.as_chunks::<2>().0 {
            for s in pair {
                let v = *s as f32 / 32768.;
                self.partial[0] = self.partial[0].min(v);
                self.partial[1] = self.partial[1].max(v);
            }
            self.count += 1;
            self.frames += 1;
            if self.count == self.bucket {
                self.peaks.push(self.partial);
                self.partial = [0.; 2];
                self.count = 0;
                if self.peaks.len() == 2048 {
                    for i in 0..1024 {
                        self.peaks[i] = [
                            self.peaks[i * 2][0].min(self.peaks[i * 2 + 1][0]),
                            self.peaks[i * 2][1].max(self.peaks[i * 2 + 1][1]),
                        ];
                    }
                    self.peaks.truncate(1024);
                    self.bucket *= 2;
                }
            }
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut peaks = self.peaks.clone();
        if self.count > 0 {
            peaks.push(self.partial);
        }
        Snapshot {
            peaks,
            frames: self.frames,
            bucket: self.bucket,
        }
    }
}
