use super::*;
#[derive(Clone, Copy)]
pub(super) struct Coeff {
    b: [f64; 3],
    a: [f64; 2],
}
impl Coeff {
    pub fn new(kind: u8, f: f64, q: f64, g: f64) -> Self {
        let w = std::f64::consts::TAU * f.clamp(10., 22000.) / SAMPLE_RATE;
        let c = w.cos();
        let alpha = w.sin() / (2. * q.max(0.1));
        let a = 10f64.powf(g / 40.);
        let (b, a0, a1, a2) = match kind {
            0 => (
                [1. + alpha * a, -2. * c, 1. - alpha * a],
                1. + alpha / a,
                -2. * c,
                1. - alpha / a,
            ),
            1 => (
                [(1. + c) / 2., -(1. + c), (1. + c) / 2.],
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            3 => (
                [(1. - c) / 2., 1. - c, (1. - c) / 2.],
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            _ => ([alpha, 0., -alpha], 1. + alpha, -2. * c, 1. - alpha),
        };
        Self {
            b: b.map(|v| v / a0),
            a: [a1 / a0, a2 / a0],
        }
    }
    pub fn response(&self, f: f64) -> f64 {
        let w = std::f64::consts::TAU * f / SAMPLE_RATE;
        let norm = |a: f64, b: f64, c: f64| {
            (a + b * w.cos() + c * (2. * w).cos()).powi(2)
                + (b * w.sin() + c * (2. * w).sin()).powi(2)
        };
        10. * (norm(self.b[0], self.b[1], self.b[2]) / norm(1., self.a[0], self.a[1]))
            .max(1e-12)
            .log10()
    }
}
#[derive(Clone, Default)]
pub(super) struct Filter {
    z: [[f64; 2]; 2],
}
impl Filter {
    pub fn sample(&mut self, c: Coeff, x: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|ch| {
            let y = c.b[0] * x[ch] + self.z[ch][0];
            self.z[ch][0] = c.b[1] * x[ch] - c.a[0] * y + self.z[ch][1];
            self.z[ch][1] = c.b[2] * x[ch] - c.a[1] * y;
            if !y.is_finite() {
                self.z[ch] = [0., 0.];
                0.
            } else {
                y
            }
        })
    }
}
fn coefficients(f: &Effect) -> Vec<Coeff> {
    match f.kind {
        Kind::Equalizer => f
            .bands
            .iter()
            .flat_map(|b| {
                let kind = match b.kind {
                    BandKind::Bell => 0,
                    BandKind::HighPass => 1,
                    BandKind::LowPass => 3,
                };
                vec![
                    Coeff::new(kind, b.frequency as f64, b.q as f64, b.gain as f64);
                    if b.kind == BandKind::Bell {
                        1
                    } else {
                        b.slope as usize / 12
                    }
                ]
            })
            .collect(),
        Kind::Clarity => vec![
            Coeff::new(0, 220., 0.7, f.values[0] as f64 * 0.03),
            Coeff::new(0, 3200., 0.85, f.values[1] as f64 * 0.055),
            Coeff::new(0, 10000., 0.7, f.values[2] as f64 * 0.035),
        ],
        Kind::DeEsser => vec![Coeff::new(2, f.values[0] as f64, 1.5, 0.)],
        _ => vec![],
    }
}
pub(super) fn response(f: &Effect, frequency: f64) -> f64 {
    if !f.enabled {
        return 0.;
    }
    let coeff = coefficients(f);
    if f.kind == Kind::Clarity {
        let w = std::f64::consts::TAU * frequency / SAMPLE_RATE;
        let mut h = (1., 0.);
        for c in &coeff {
            let n = (
                c.b[0] + c.b[1] * w.cos() + c.b[2] * (2. * w).cos(),
                -c.b[1] * w.sin() - c.b[2] * (2. * w).sin(),
            );
            let d = (
                1. + c.a[0] * w.cos() + c.a[1] * (2. * w).cos(),
                -c.a[0] * w.sin() - c.a[1] * (2. * w).sin(),
            );
            let norm = d.0 * d.0 + d.1 * d.1;
            let v = (
                (n.0 * d.0 + n.1 * d.1) / norm,
                (n.1 * d.0 - n.0 * d.1) / norm,
            );
            h = (h.0 * v.0 - h.1 * v.1, h.0 * v.1 + h.1 * v.0);
        }
        let mix = f.values[3] as f64 / 100.;
        return db(((1. - mix + h.0 * mix).powi(2) + (h.1 * mix).powi(2)).sqrt());
    }
    let value: f64 = coeff.iter().map(|c| c.response(frequency)).sum();
    if f.kind == Kind::Equalizer {
        value + f.values[0] as f64
    } else {
        value
    }
}
fn smooth(current: f64, target: f64, ms: f64) -> f64 {
    let a = (-1. / (SAMPLE_RATE * ms / 1000.)).exp();
    target + (current - target) * a
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Meter {
    pub input: f32,
    pub output: f32,
    pub reduction: f32,
    pub spectrum: Option<Spectrum>,
    pub history: Option<super::History>,
}
#[derive(Clone)]
struct Stage {
    effect: Effect,
    c: Vec<Coeff>,
    f: Vec<Filter>,
    envelope: f64,
    level: f64,
    applied: f64,
    meter: Meter,
    spectrum: Option<super::spectrum::LiveSpectrum>,
    history: Option<super::history::LiveHistory>,
}
impl Stage {
    fn new(effect: Effect) -> Self {
        let c = coefficients(&effect);
        let f = vec![Filter::default(); c.len()];
        Self {
            effect,
            c,
            f,
            envelope: 0.,
            level: 0.,
            applied: 1.,
            meter: Meter::default(),
            spectrum: None,
            history: None,
        }
    }
    fn sample(&mut self, x: [f64; 2]) -> [f64; 2] {
        if !self.effect.enabled {
            self.meter.input = self.meter.input.max(x[0].abs().max(x[1].abs()) as f32);
            self.meter.output = self.meter.input;
            if let Some(s) = &mut self.spectrum {
                s.push(x, x);
            }
            if let Some(h) = &mut self.history {
                h.push(x, x);
            }
            return x;
        }
        let v = self.effect.values.map(|v| v as f64);
        let peak = x[0].abs().max(x[1].abs());
        let mut y = x;
        match self.effect.kind {
            Kind::Equalizer | Kind::Clarity => {
                for (f, c) in self.f.iter_mut().zip(&self.c) {
                    y = f.sample(*c, y);
                }
                if self.effect.kind == Kind::Equalizer {
                    y = y.map(|s| s * gain(v[0]));
                }
                if self.effect.kind == Kind::Clarity {
                    y = std::array::from_fn(|i| x[i] + (y[i] - x[i]) * v[3] / 100.);
                }
            }
            Kind::Compressor => {
                self.envelope = smooth(
                    self.envelope,
                    peak,
                    if peak > self.envelope { v[2] } else { v[3] },
                );
                let above = (db(self.envelope) - v[0]).max(0.);
                self.applied = gain(-above * (1. - 1. / v[1]));
                y = x.map(|s| s * self.applied);
            }
            Kind::DeEsser => {
                let band = self.f[0].sample(self.c[0], x);
                let p = band[0].abs().max(band[1].abs());
                self.envelope = smooth(self.envelope, p, if p > self.envelope { 1.5 } else { 65. });
                let reduce = ((db(self.envelope) - v[1]) * (0.25 + v[3] / 100.)).clamp(0., v[2]);
                self.applied = gain(-reduce);
                y = std::array::from_fn(|i| x[i] - band[i] * (1. - self.applied));
            }
            Kind::Leveler => {
                let energy = (x[0] * x[0] + x[1] * x[1]) * 0.5;
                self.level = smooth(self.level, energy, 100.);
                let rms = self.level.sqrt();
                let target = if db(rms) < -50. {
                    1.
                } else {
                    gain(((v[0] - db(rms)) * v[3] / 100.).clamp(-v[1], v[1]))
                };
                self.applied = smooth(self.applied, target, 2000. - v[2] * 17.);
                y = x.map(|s| s * self.applied);
            }
            Kind::Limiter => {
                let input = gain(v[2]);
                let ceiling = gain(v[0]);
                let p = peak * input;
                let target = if p > ceiling { ceiling / p } else { 1. };
                self.applied = if target < self.applied {
                    target
                } else {
                    smooth(self.applied, target, v[1])
                };
                y = x.map(|s| (s * input * self.applied).clamp(-ceiling, ceiling));
            }
        }
        self.meter.input = self.meter.input.max(peak as f32);
        self.meter.output = self.meter.output.max(y[0].abs().max(y[1].abs()) as f32);
        self.meter.reduction = self.meter.reduction.max((-db(self.applied)).max(0.) as f32);
        if let Some(s) = &mut self.spectrum {
            s.push(x, y);
        }
        if let Some(h) = &mut self.history {
            h.push(x, y);
        }
        y.map(|s| {
            if s.is_finite() {
                s.clamp(-64., 64.)
            } else {
                0.
            }
        })
    }
}
pub struct Processor {
    chain: Chain,
    stages: Vec<Stage>,
    old: Option<(Vec<Stage>, bool, f32)>,
    transition: usize,
    pub meters: Vec<Meter>,
}
impl Processor {
    pub fn new(chain: &Chain) -> Result<Self> {
        chain.validate()?;
        Ok(Self {
            chain: chain.clone(),
            stages: chain.effects.iter().cloned().map(Stage::new).collect(),
            old: None,
            transition: 0,
            meters: vec![Meter::default(); chain.effects.len()],
        })
    }
    pub fn set_visualization(&mut self, id: Option<u64>) {
        for s in &mut self.stages {
            if id.is_none() {
                s.spectrum = None;
                s.history = None;
                continue;
            }
            if id.is_some() && s.history.is_none() {
                s.history = Some(Default::default());
            }
            if id.is_some()
                && matches!(s.effect.kind, Kind::Equalizer | Kind::Clarity)
                && s.spectrum.is_none()
            {
                s.spectrum = Some(Default::default());
            }
        }
    }
    pub fn matches(&self, chain: &Chain) -> bool {
        &self.chain == chain
    }
    pub fn reconfigure(&mut self, chain: &Chain) -> Result<()> {
        chain.validate()?;
        if self.matches(chain) {
            return Ok(());
        }
        let mut old = self.stages.clone();
        for s in &mut old {
            s.spectrum = None;
            s.history = None;
        }
        self.old = Some((old, self.chain.bypass, self.chain.mix));
        self.transition = 480;
        self.stages = chain
            .effects
            .iter()
            .map(|effect| {
                if let Some(stage) = self
                    .stages
                    .iter()
                    .find(|s| s.effect.id == effect.id && s.effect.kind == effect.kind)
                {
                    let mut stage = stage.clone();
                    let changed_topology = stage.effect.bands.len() != effect.bands.len()
                        || stage
                            .effect
                            .bands
                            .iter()
                            .zip(&effect.bands)
                            .any(|(a, b)| a.kind != b.kind || a.slope != b.slope);
                    stage.effect = effect.clone();
                    stage.c = coefficients(effect);
                    if changed_topology {
                        stage.f = vec![Filter::default(); stage.c.len()];
                    } else {
                        stage.f.resize(stage.c.len(), Filter::default());
                    }
                    stage
                } else {
                    Stage::new(effect.clone())
                }
            })
            .collect();
        self.chain = chain.clone();
        self.meters.resize(chain.effects.len(), Meter::default());
        Ok(())
    }
    pub fn process(&mut self, samples: &mut [[f64; 2]]) {
        for stage in &mut self.stages {
            stage.meter = Meter::default();
        }
        for sample in samples {
            let original = *sample;
            if !self.chain.bypass {
                for stage in &mut self.stages {
                    *sample = stage.sample(*sample);
                }
            } else {
                for stage in &mut self.stages {
                    let peak = sample[0].abs().max(sample[1].abs()) as f32;
                    stage.meter.input = stage.meter.input.max(peak);
                    stage.meter.output = stage.meter.input;
                    if let Some(spectrum) = &mut stage.spectrum {
                        spectrum.push(*sample, *sample);
                    }
                    if let Some(h) = &mut stage.history {
                        h.push(*sample, *sample);
                    }
                }
            }
            *sample = std::array::from_fn(|i| {
                original[i] + (sample[i] - original[i]) * self.chain.mix as f64
            });
            if self.transition > 0 {
                if let Some((old, bypass, mix)) = &mut self.old {
                    let mut before = original;
                    if !*bypass {
                        for stage in old {
                            before = stage.sample(before);
                        }
                    }
                    before = std::array::from_fn(|i| {
                        original[i] + (before[i] - original[i]) * *mix as f64
                    });
                    let t = (481 - self.transition) as f64 / 480.;
                    *sample = std::array::from_fn(|i| before[i] * (1. - t) + sample[i] * t);
                }
                self.transition -= 1;
                if self.transition == 0 {
                    self.old = None;
                }
            }
        }
        for (m, s) in self.meters.iter_mut().zip(&self.stages) {
            *m = s.meter;
            m.spectrum = s.spectrum.as_ref().map(|s| s.value);
            m.history = s.history.as_ref().map(|h| h.snapshot());
        }
    }
}
