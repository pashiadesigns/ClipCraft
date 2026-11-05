//! Editable, bounded track processors. Originals are never rewritten.
//! Biquad coefficients use the standard bilinear-transform equations documented
//! in https://www.w3.org/TR/audio-eq-cookbook/ (independent implementation).
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
pub mod analysis;
mod dsp;
mod history;
mod spectrum;
pub use dsp::{Meter, Processor};
pub use history::{History, Point as HistoryPoint};
pub use spectrum::Spectrum;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Clip(u64),
    Track(u64),
}
impl super::editor::Project {
    pub fn effect_chain(&self, target: Target) -> Option<&Chain> {
        match target {
            Target::Clip(id) => self.clip_effects.get(&id),
            Target::Track(id) => self.track_effects.get(&id),
        }
    }
    pub fn set_effect_chain(&mut self, target: Target, chain: Chain) {
        match target {
            Target::Clip(id) => {
                self.clip_effects.insert(id, chain);
            }
            Target::Track(id) => {
                self.track_effects.insert(id, chain);
            }
        }
    }
}
pub const MAX_EQ_BANDS: usize = 16;
pub const SAMPLE_RATE: f64 = 48_000.;
pub const MAX_EFFECTS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Equalizer,
    Compressor,
    Clarity,
    DeEsser,
    Leveler,
    Limiter,
}
impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Equalizer,
        Self::Compressor,
        Self::Clarity,
        Self::DeEsser,
        Self::Leveler,
        Self::Limiter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Equalizer => "Parametric EQ",
            Self::Compressor => "Compressor",
            Self::Clarity => "Vocal Clarity",
            Self::DeEsser => "De-esser",
            Self::Leveler => "Voice Leveler",
            Self::Limiter => "Output Limiter",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Equalizer => "EQ",
            Self::Clarity => "Clarity",
            Self::Leveler => "Leveler",
            Self::Limiter => "Limiter",
            _ => self.label(),
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Equalizer => "chart-no-axes-combined",
            Self::Compressor => "minimize-2",
            Self::Clarity => "sparkles",
            Self::DeEsser => "audio-lines",
            Self::Leveler => "equal",
            Self::Limiter => "shield-check",
        }
    }
    pub fn parameters(self) -> &'static [Parameter] {
        match self {
            Self::Equalizer => const { &[Parameter::new("Output", -12., 12., 0.1, "dB")] },
            Self::Compressor => {
                const {
                    &[
                        Parameter::new("Threshold", -48., 0., 0.5, "dB"),
                        Parameter::new("Ratio", 1., 8., 0.1, ":1"),
                        Parameter::new("Attack", 1., 80., 1., "ms"),
                        Parameter::new("Release", 30., 500., 5., "ms"),
                    ]
                }
            }
            Self::Clarity => {
                const {
                    &[
                        Parameter::new("Body", 0., 100., 1., "%"),
                        Parameter::new("Presence", 0., 100., 1., "%"),
                        Parameter::new("Air", 0., 100., 1., "%"),
                        Parameter::new("Mix", 0., 100., 1., "%"),
                    ]
                }
            }
            Self::DeEsser => {
                const {
                    &[
                        Parameter::new("Frequency", 2500., 11000., 100., "Hz"),
                        Parameter::new("Threshold", -48., -6., 1., "dB"),
                        Parameter::new("Max reduction", 0., 12., 0.5, "dB"),
                        Parameter::new("Sensitivity", 0., 100., 1., "%"),
                    ]
                }
            }
            Self::Leveler => {
                const {
                    &[
                        Parameter::new("Target RMS", -30., -12., 0.5, "dBFS"),
                        Parameter::new("Max gain", 0., 12., 0.5, "dB"),
                        Parameter::new("Response", 10., 100., 1., "%"),
                        Parameter::new("Strength", 0., 100., 1., "%"),
                    ]
                }
            }
            Self::Limiter => {
                const {
                    &[
                        Parameter::new("Ceiling", -6., -0.1, 0.1, "dBFS"),
                        Parameter::new("Release", 20., 250., 5., "ms"),
                        Parameter::new("Input gain", -6., 12., 0.5, "dB"),
                    ]
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
pub struct Parameter {
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: &'static str,
}
impl Parameter {
    const fn new(label: &'static str, min: f32, max: f32, step: f32, unit: &'static str) -> Self {
        Self {
            label,
            min,
            max,
            step,
            unit,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BandKind {
    Bell,
    HighPass,
    LowPass,
}
impl BandKind {
    pub const ALL: [Self; 3] = [Self::Bell, Self::HighPass, Self::LowPass];
    pub fn label(self) -> &'static str {
        match self {
            Self::Bell => "Bell",
            Self::HighPass => "Low Cut (high-pass)",
            Self::LowPass => "High Cut (low-pass)",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub kind: BandKind,
    pub slope: u8,
    pub frequency: f32,
    pub gain: f32,
    pub q: f32,
}
impl Band {
    pub fn new(kind: BandKind, frequency: f32) -> Self {
        Self {
            kind,
            slope: 12,
            frequency,
            gain: 0.,
            q: std::f32::consts::FRAC_1_SQRT_2,
        }
    }
    pub fn bell(frequency: f32, q: f32) -> Self {
        Self {
            q,
            ..Self::new(BandKind::Bell, frequency)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: u64,
    pub kind: Kind,
    pub enabled: bool,
    pub values: [f32; 4],
    pub bands: Vec<Band>,
}
impl Effect {
    pub fn new(id: u64, kind: Kind) -> Self {
        Self {
            id,
            kind,
            enabled: true,
            values: match kind {
                Kind::Equalizer => [0.; 4],
                Kind::Compressor => [-21., 3., 12., 110.],
                Kind::Clarity => [25., 35., 18., 100.],
                Kind::DeEsser => [6500., -25., 4., 50.],
                Kind::Leveler => [-22., 6., 40., 65.],
                Kind::Limiter => [-1., 80., 0., 0.],
            },
            bands: vec![
                Band::bell(180., 0.8),
                Band::bell(900., 1.),
                Band::bell(3200., 1.1),
            ],
        }
    }
    pub fn validate(&self) -> bool {
        self.id > 0
            && self.bands.len() <= MAX_EQ_BANDS
            && self.values.iter().all(|v| v.is_finite())
            && self
                .kind
                .parameters()
                .iter()
                .zip(self.values)
                .all(|(p, v)| (p.min..=p.max).contains(&v))
            && self.bands.iter().all(|b| {
                [12, 24].contains(&b.slope)
                    && b.frequency.is_finite()
                    && (20. ..=20000.).contains(&b.frequency)
                    && b.gain.is_finite()
                    && (-12. ..=12.).contains(&b.gain)
                    && b.q.is_finite()
                    && (0.3..=5.).contains(&b.q)
            })
    }
    pub fn response_db(&self, frequency: f64) -> f64 {
        dsp::response(self, frequency)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    #[serde(default = "full_mix")]
    pub mix: f32,
    pub bypass: bool,
    pub effects: Vec<Effect>,
}
fn full_mix() -> f32 {
    1.
}
impl Default for Chain {
    fn default() -> Self {
        Self {
            bypass: false,
            mix: 1.,
            effects: vec![],
        }
    }
}
impl Chain {
    pub fn narration() -> Self {
        Self {
            bypass: false,
            mix: 1.,
            effects: Kind::ALL[..6]
                .iter()
                .enumerate()
                .map(|(i, k)| Effect::new(i as u64 + 1, *k))
                .collect(),
        }
    }
    pub fn add(&mut self, kind: Kind) -> Result<u64> {
        if self.effects.len() >= MAX_EFFECTS {
            return Err(Error::Invalid(format!(
                "Keep at most {MAX_EFFECTS} effects on one track."
            )));
        }
        let id = self
            .effects
            .iter()
            .map(|f| f.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Effect identifier limit reached.".into()))?;
        self.effects.push(Effect::new(id, kind));
        Ok(id)
    }
    pub fn validate(&self) -> Result<()> {
        let ids: std::collections::BTreeSet<_> = self.effects.iter().map(|f| f.id).collect();
        if !self.mix.is_finite()
            || !(0. ..=1.).contains(&self.mix)
            || self.effects.len() > MAX_EFFECTS
            || ids.len() != self.effects.len()
            || self.effects.iter().any(|f| !f.validate())
        {
            return Err(Error::Invalid("Invalid audio effects. Use finite values within the displayed ranges and at most 12 effects per track.".into()));
        }
        Ok(())
    }
}
pub fn db(value: f64) -> f64 {
    20. * value.abs().max(1e-6).log10()
}
pub fn gain(db: f64) -> f64 {
    10f64.powf(db / 20.)
}

#[cfg(test)]
mod tests;
