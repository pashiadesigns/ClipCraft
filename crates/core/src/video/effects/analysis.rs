//! Bounded local acoustic analysis. Estimates audible activity and spectral balance;
//! it does not recognize speech or recover information missing from the recording.
use super::{
    dsp::{Coeff, Filter},
    *,
};
pub const FREQUENCIES: [f64; 18] = [
    60., 85., 120., 170., 240., 340., 480., 680., 960., 1360., 1920., 2720., 3840., 5440., 6800.,
    8600., 10800., 13600.,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Character {
    Natural,
    Warm,
    Bright,
}
impl Character {
    pub const ALL: [Self; 3] = [Self::Natural, Self::Warm, Self::Bright];
    pub fn label(self) -> &'static str {
        match self {
            Self::Natural => "Natural & clear",
            Self::Warm => "Warm & smooth",
            Self::Bright => "Bright & present",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Report {
    pub seconds: f64,
    pub speech_seconds: f64,
    pub rms_db: f64,
    pub peak_db: f64,
    pub quiet_db: f64,
    pub dynamic_db: f64,
    pub clipped_fraction: f64,
    pub bands: [f64; 6],
    pub spectrum: [f64; 18],
    pub sibilant_peak_db: f64,
    pub warnings: Vec<String>,
    pub usable: bool,
}
struct Block {
    level: f64,
    frames: usize,
    bands: [f64; 18],
}
pub struct Analyzer {
    filters: [[Filter; 2]; 18],
    coeff: [Coeff; 18],
    frames: usize,
    peak: f64,
    clipped: usize,
    block: f64,
    block_frames: usize,
    block_bands: [f64; 18],
    blocks: Vec<Block>,
}
impl Default for Analyzer {
    fn default() -> Self {
        Self {
            filters: std::array::from_fn(|_| std::array::from_fn(|_| Filter::default())),
            coeff: FREQUENCIES.map(|f| Coeff::new(2, f, 1.4, 0.)),
            frames: 0,
            peak: 0.,
            clipped: 0,
            block: 0.,
            block_frames: 0,
            block_bands: [0.; 18],
            blocks: vec![],
        }
    }
}
fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get((values.len().saturating_sub(1) as f64 * fraction) as usize)
        .copied()
        .unwrap_or(-120.)
}
impl Analyzer {
    pub const MAX_FRAMES: usize = 120 * 48000;
    fn finish_block(&mut self) {
        if self.block_frames == 0 {
            return;
        }
        let n = self.block_frames as f64;
        self.blocks.push(Block {
            level: db((self.block / n).sqrt()),
            frames: self.block_frames,
            bands: self.block_bands.map(|b| b / n),
        });
        self.block = 0.;
        self.block_frames = 0;
        self.block_bands = [0.; 18];
    }
    pub fn push(&mut self, samples: &[i16]) {
        for x in samples
            .as_chunks::<2>()
            .0
            .iter()
            .take(Self::MAX_FRAMES.saturating_sub(self.frames))
        {
            let x = [x[0] as f64 / 32768., x[1] as f64 / 32768.];
            let energy = (x[0] * x[0] + x[1] * x[1]) * 0.5;
            let p = x[0].abs().max(x[1].abs());
            self.peak = self.peak.max(p);
            self.clipped += usize::from(p >= 0.999);
            self.block += energy;
            self.block_frames += 1;
            self.frames += 1;
            for ((filter, c), sum) in self
                .filters
                .iter_mut()
                .zip(self.coeff)
                .zip(&mut self.block_bands)
            {
                // Cascaded bands limit low-frequency leakage into presence measurements.
                let y = filter[0].sample(c, x);
                let y = filter[1].sample(c, y);
                *sum += (y[0] * y[0] + y[1] * y[1]) * 0.5;
            }
            if self.block_frames == 960 {
                self.finish_block();
            }
        }
    }
    pub fn report(mut self) -> Report {
        self.finish_block();
        let mut levels: Vec<_> = self.blocks.iter().map(|b| b.level).collect();
        let quiet = percentile(&mut levels, 0.1);
        let gate = (percentile(&mut levels, 0.8) - 28.).max(-55.);
        let active: Vec<_> = self.blocks.iter().filter(|b| b.level > gate).collect();
        let frames = active.iter().map(|b| b.frames).sum::<usize>();
        let spectrum = std::array::from_fn(|i| {
            active
                .iter()
                .map(|b| b.bands[i] * b.frames as f64)
                .sum::<f64>()
                / frames.max(1) as f64
        });
        let mut voiced: Vec<_> = active.iter().map(|b| b.level).collect();
        let dynamic = (percentile(&mut voiced, 0.9) - percentile(&mut voiced, 0.2)).max(0.);
        let rms = db((active
            .iter()
            .map(|b| 10f64.powf(b.level / 10.) * b.frames as f64)
            .sum::<f64>()
            / frames.max(1) as f64)
            .sqrt());
        let mut sibilance: Vec<_> = active
            .iter()
            .map(|b| {
                10. * b.bands[13..17]
                    .iter()
                    .copied()
                    .fold(0., f64::max)
                    .max(1e-12)
                    .log10()
            })
            .collect();
        let sibilant_peak_db = percentile(&mut sibilance, 0.9);
        let speech = frames as f64 / SAMPLE_RATE;
        let usable = speech >= 1.;
        let mut warnings = vec![];
        if !usable {
            warnings.push("Record at least a second of audible speech, then analyze again.".into());
        }
        if self.frames < 5 * 48000 {
            warnings
                .push("A longer spoken passage will produce more representative settings.".into());
        }
        if self.clipped > 0 {
            warnings.push("Clipped peaks were found. Lower the microphone input level and record again; effects cannot restore clipped detail.".into());
        }
        if quiet > -35. {
            warnings.push("There are few quiet passages. Check room noise by ear before adding presence or gain.".into());
        }
        Report {
            seconds: self.frames as f64 / SAMPLE_RATE,
            speech_seconds: speech,
            rms_db: rms,
            peak_db: db(self.peak),
            quiet_db: quiet,
            dynamic_db: dynamic,
            clipped_fraction: self.clipped as f64 / self.frames.max(1) as f64,
            bands: [
                spectrum[0],
                spectrum[4],
                spectrum[8],
                spectrum[11],
                spectrum[14],
                spectrum[16],
            ],
            spectrum,
            sibilant_peak_db,
            warnings,
            usable,
        }
    }
}
impl Report {
    pub fn suggest(
        &self,
        character: Character,
        strength: f32,
        existing: &Chain,
        preserve_bypassed: bool,
    ) -> Result<Chain> {
        if !self.usable {
            return Err(Error::Invalid(
                "Record more audible speech before applying automatic settings.".into(),
            ));
        }
        let amount = strength.clamp(10., 85.) / 85.;
        let level = |i: usize| (10. * self.spectrum[i].max(1e-12).log10()) as f32;
        let reference = 10.
            * ((self.spectrum[7] + self.spectrum[8] + self.spectrum[9]) / 3.)
                .max(1e-12)
                .log10() as f32;
        let strongest = |range: std::ops::Range<usize>| {
            range
                .max_by(|a, b| level(*a).total_cmp(&level(*b)))
                .unwrap()
        };
        let mud = strongest(4..7);
        let muddy = (level(mud) - reference - 5.).max(0.);
        let boxy = (5..9)
            .max_by(|a, b| {
                let prominence = |i: usize| level(i) - (level(i - 1) + level(i + 1)) * 0.5;
                prominence(*a).total_cmp(&prominence(*b))
            })
            .unwrap();
        let box_excess = (level(boxy) - (level(boxy - 1) + level(boxy + 1)) * 0.5 - 2.).max(0.);
        let presence = strongest(10..13);
        let target = match character {
            Character::Natural => -7.,
            Character::Warm => -10.,
            Character::Bright => -4.,
        };
        let presence_deficit = (reference + target - level(presence)).clamp(-5., 8.);
        let sibilant = strongest(13..17);
        let sharp = (level(sibilant) - reference + 12.).max(0.);
        let harsh = strongest(11..14);
        let harshness = (level(harsh) - reference + 2.).max(0.);
        let air_deficit = (reference - 20. - level(16)).clamp(0., 6.);
        let crest = (self.peak_db - self.rms_db) as f32;
        let body = (reference - level(4) + 3.).clamp(0., 9.);
        let rumble = (level(0) - level(3) + 3.).clamp(0., 12.);
        let mut chain = Chain::narration();
        for effect in &mut chain.effects {
            match effect.kind {
                Kind::Equalizer => {
                    let bell = |frequency: f32, gain: f32, q: f32| Band {
                        kind: BandKind::Bell,
                        frequency,
                        gain: gain * amount,
                        q,
                        slope: 12,
                    };
                    effect.bands = vec![
                        Band::new(
                            BandKind::HighPass,
                            (50. + rumble * 4. * amount).clamp(45., 100.),
                        ),
                        bell(FREQUENCIES[mud] as f32, -(muddy * 0.65).min(9.), 0.85),
                        bell(FREQUENCIES[boxy] as f32, -(box_excess * 0.55).min(5.), 1.8),
                        bell(
                            FREQUENCIES[presence] as f32,
                            (presence_deficit * 0.65).clamp(-3., 5.),
                            0.8,
                        ),
                        bell(FREQUENCIES[harsh] as f32, -(harshness * 0.4).min(4.), 1.5),
                        bell(
                            10800.,
                            (air_deficit * 0.4).min(if character == Character::Warm {
                                0.8
                            } else {
                                2.5
                            }),
                            0.7,
                        ),
                    ];
                }
                Kind::Compressor => {
                    effect.values = [
                        (self.rms_db as f32 + 4. - 6. * amount).clamp(-42., -8.),
                        (1. + ((crest - 7.).max(0.) * 0.22 + self.dynamic_db as f32 * 0.04)
                            * amount)
                            .clamp(1.1, 4.5),
                        (8. + crest * 1.1).clamp(5., 35.),
                        (80. + self.dynamic_db as f32 * 8.).clamp(60., 280.),
                    ];
                }
                Kind::Clarity => {
                    effect.values = [
                        ((if character == Character::Warm {
                            20.
                        } else {
                            8.
                        }) + body * 2.
                            - muddy)
                            .clamp(0., 45.)
                            * amount,
                        ((if character == Character::Bright {
                            25.
                        } else {
                            12.
                        }) + presence_deficit.max(0.) * 2.
                            - harshness)
                            .clamp(0., 50.)
                            * amount,
                        (air_deficit * 2.).min(if character == Character::Warm {
                            4.
                        } else {
                            14.
                        }) * amount,
                        100.,
                    ];
                }
                Kind::DeEsser => {
                    effect.values = [
                        FREQUENCIES[sibilant].min(11000.) as f32,
                        (self.sibilant_peak_db as f32 - 2. - 5. * amount).clamp(-48., -6.),
                        (sharp * 0.5 + 1.).clamp(1., 8.) * amount,
                        (25. + sharp * 2. + 20. * amount).clamp(20., 90.),
                    ];
                }
                Kind::Leveler => {
                    let target = -24. + 4. * amount;
                    effect.values = [
                        target,
                        ((target - self.rms_db as f32).max(0.) + self.dynamic_db as f32 * 0.2)
                            .clamp(0., 12.)
                            * amount,
                        (25. + self.dynamic_db as f32 * 2.).clamp(20., 65.),
                        30. + 55. * amount,
                    ];
                }
                Kind::Limiter => {
                    effect.values = [
                        -1.,
                        (60. + self.dynamic_db as f32 * 4.).clamp(40., 180.),
                        0.,
                        0.,
                    ]
                }
            }
        }
        if preserve_bypassed {
            let preserved: Vec<_> = existing
                .effects
                .iter()
                .filter(|e| !e.enabled)
                .cloned()
                .collect();
            chain
                .effects
                .retain(|e| !preserved.iter().any(|old| old.kind == e.kind));
            chain
                .effects
                .truncate(MAX_EFFECTS.saturating_sub(preserved.len()));
            chain.effects.extend(preserved);
            for (i, e) in chain.effects.iter_mut().enumerate() {
                e.id = i as u64 + 1;
            }
        }
        chain.validate()?;
        Ok(chain)
    }
}
