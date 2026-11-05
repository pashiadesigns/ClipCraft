//! Bounded stereo PCM alignment. Late samples cannot shift already encoded time.
use crate::{Error, Result};
use std::collections::VecDeque;
#[derive(Default)]
pub struct Track {
    start: u64,
    samples: VecDeque<i16>,
    anchor: Option<(u64, u64)>,
}
impl Track {
    pub fn reset(&mut self, frame: u64) {
        self.start = frame;
        self.samples.clear();
        self.anchor = None;
    }
    /// Anchor once to the shared recording clock, then use sample positions.
    /// Packet timestamp jitter must not insert/drop samples inside a stream.
    pub fn push_positioned(&mut self, position: u64, at: u64, samples: &[i16]) -> Result<u64> {
        let (origin, timeline) = *self.anchor.get_or_insert((position, at));
        let aligned = timeline.saturating_add(position.saturating_sub(origin));
        self.push(aligned, samples)?;
        Ok(aligned)
    }
    pub fn push(&mut self, mut at: u64, mut samples: &[i16]) -> Result<()> {
        if !samples.len().is_multiple_of(2) {
            return Err(Error::Invalid("Audio packet is not stereo PCM.".into()));
        }
        if at < self.start {
            let skip = (self.start.saturating_sub(at).saturating_mul(2)).min(samples.len() as u64)
                as usize;
            samples = &samples[skip..];
            at = self.start;
        }
        if samples.is_empty() {
            return Ok(());
        }
        let end = self.start + self.samples.len() as u64 / 2;
        if at < end {
            let skip =
                (end.saturating_sub(at).saturating_mul(2)).min(samples.len() as u64) as usize;
            samples = &samples[skip..];
            at = end;
        }
        let gap = at.saturating_sub(end);
        if gap
            .saturating_mul(2)
            .saturating_add(self.samples.len() as u64)
            .saturating_add(samples.len() as u64)
            > 192_000
        {
            return Err(Error::Invalid("Audio processing fell more than two seconds behind. Finish this recording and use a lower frame rate.".into()));
        }
        self.samples
            .extend(std::iter::repeat_n(0, (gap * 2) as usize));
        self.samples.extend(samples);
        Ok(())
    }
    pub fn drain(&mut self, frames: usize) -> Vec<i16> {
        let out = (0..frames * 2)
            .map(|_| self.samples.pop_front().unwrap_or(0))
            .collect();
        self.start += frames as u64;
        out
    }
}
pub fn mix(a: &[i16], b: &[i16]) -> Result<Vec<i16>> {
    if a.len() != b.len() {
        return Err(Error::Invalid("Audio tracks must share a timeline.".into()));
    }
    Ok(a.iter()
        .zip(b)
        .map(|(&a, &b)| (a as i32 + b as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16)
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positioned_packets_preserve_every_sample_despite_timestamp_jitter_and_silence() {
        let mut t = Track::default();
        let mut expected = vec![0; 200];
        for n in 0..100u64 {
            let samples: Vec<_> = (0..960)
                .map(|i| ((n * 960 + i) % 30000) as i16 + 1)
                .collect();
            expected.extend_from_slice(&samples);
            t.push_positioned(
                n * 480,
                100 + n * 480 + if n % 2 == 0 { 0 } else { 4 },
                &samples,
            )
            .unwrap();
        }
        assert_eq!(t.drain(expected.len() / 2), expected);
        t.reset(0);
        t.push_positioned(1000, 0, &[7; 960]).unwrap();
        t.push_positioned(1960, 960, &[8; 960]).unwrap();
        let samples = t.drain(1440);
        assert_eq!(&samples[960..1920], &[0; 960]);
        assert_eq!(&samples[1920..], &[8; 960]);
    }
    #[test]
    fn gaps_late_packets_pause_reset_and_clipping_are_bounded() {
        let mut t = Track::default();
        t.push(2, &[3, 4, 5, 6]).unwrap();
        assert_eq!(t.drain(3), [0, 0, 0, 0, 3, 4]);
        t.push(0, &[9; 6]).unwrap();
        assert_eq!(t.drain(2), [5, 6, 0, 0]);
        t.reset(100);
        t.push(101, &[7, 8]).unwrap();
        assert_eq!(t.drain(2), [0, 0, 7, 8]);
        assert!(t.push(1_000_000, &[1, 2]).is_err());
        assert_eq!(
            mix(&[32_000, -32_000], &[32_000, -32_000]).unwrap(),
            [32767, -32768]
        );
    }
}
