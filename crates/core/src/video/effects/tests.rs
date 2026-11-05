use super::*;
#[test]
fn clarity_response_tracks_dry_wet_mix() {
    let mut f = Effect::new(1, Kind::Clarity);
    f.values = [80., 80., 80., 0.];
    assert!(f.response_db(3200.).abs() < 1e-8);
    f.values[3] = 50.;
    let half = f.response_db(3200.);
    f.values[3] = 100.;
    assert!(half > 0. && half < f.response_db(3200.));
}

#[test]
fn live_history_is_bounded_measured_and_does_not_change_audio() {
    for kind in Kind::ALL {
        let mut chain = Chain::default();
        chain.effects.push(Effect::new(1, kind));
        let mut measured = Processor::new(&chain).unwrap();
        measured.set_visualization(Some(1));
        let mut plain = Processor::new(&chain).unwrap();
        for block in 0..130 {
            let mut input: Vec<_> = (0..2400)
                .map(|i| {
                    let t = (block * 2400 + i) as f64 / 48000.;
                    let v =
                        (t * std::f64::consts::TAU * 220.).sin() * (0.3 + 0.25 * (t * 4.).sin());
                    [v, -v]
                })
                .collect();
            let mut other = input.clone();
            measured.process(&mut input);
            plain.process(&mut other);
            assert_eq!(input, other);
        }
        let h = measured.meters[0].history.unwrap();
        assert_eq!(h.len, 250);
        assert!(
            h.points
                .iter()
                .all(|p| p.input.is_finite() && p.output.is_finite() && p.change.is_finite())
        );
        assert!(h.points.iter().any(|p| p.input > 0.4));
        if kind == Kind::Compressor {
            assert!(
                h.points
                    .iter()
                    .any(|p| p.output < p.input * 0.7 && p.change < -3.)
            );
        }
        measured.set_visualization(None);
        measured.process(&mut [[0.; 2]; 960]);
        assert!(measured.meters[0].history.is_none());
    }
}
#[test]
fn history_is_chunk_invariant_and_bypass_is_honest() {
    let mut chain = Chain::narration();
    chain.bypass = true;
    let mut a = Processor::new(&chain).unwrap();
    let mut b = Processor::new(&chain).unwrap();
    a.set_visualization(Some(2));
    b.set_visualization(Some(2));
    let mut samples = vec![[0.4, -0.4]; 48000];
    let mut other = samples.clone();
    a.process(&mut samples);
    for block in other.chunks_mut(731) {
        b.process(block);
    }
    let ah = a.meters[1].history.unwrap();
    let bh = b.meters[1].history.unwrap();
    assert_eq!(ah.len, bh.len);
    for (a, b) in ah.points[..ah.len].iter().zip(&bh.points[..bh.len]) {
        assert_eq!(a.input, a.output);
        assert_eq!(a.change, 0.);
        assert_eq!(a.input, b.input);
        assert_eq!(a.output_rms, b.output_rms);
    }
}

#[test]
fn editable_eq_cut_filters_and_band_limits_are_persisted_and_audible() {
    let mut eq = Effect::new(1, Kind::Equalizer);
    eq.bands = vec![
        Band::new(BandKind::HighPass, 120.),
        Band::new(BandKind::LowPass, 6000.),
    ];
    let at_30 = eq.response_db(30.);
    assert!(at_30 < -23.);
    assert!(eq.response_db(1000.).abs() < 0.1);
    assert!(eq.response_db(18000.) < -20.);
    eq.bands[0].slope = 24;
    assert!(eq.response_db(30.) < at_30 * 1.9);
    let roundtrip: Effect = serde_json::from_str(&serde_json::to_string(&eq).unwrap()).unwrap();
    assert_eq!(roundtrip, eq);
    eq.bands.resize(MAX_EQ_BANDS, Band::bell(1000., 1.));
    assert!(eq.validate());
    eq.bands.push(Band::bell(1000., 1.));
    assert!(!eq.validate());
}

#[test]
fn live_spectrum_does_not_change_samples_and_tracks_eq_output() {
    let mut chain = Chain::default();
    chain.add(Kind::Equalizer).unwrap();
    chain.effects[0].bands = vec![Band::bell(1500., 1.)];
    chain.effects[0].bands[0].gain = 6.;
    let mut dry = Processor::new(&chain).unwrap();
    let mut metered = Processor::new(&chain).unwrap();
    metered.set_visualization(Some(1));
    let mut samples: Vec<_> = (0..12000)
        .map(|i| {
            let v = (std::f64::consts::TAU * 1500. * i as f64 / SAMPLE_RATE).sin() * 0.1;
            [v, -v]
        })
        .collect();
    let mut other = samples.clone();
    dry.process(&mut samples);
    metered.process(&mut other);
    assert_eq!(samples, other);
    let s = metered.meters[0].spectrum.unwrap();
    let i = s
        .input
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    assert!((s.output[i] - s.input[i] - 6.).abs() < 0.1);
    metered.set_visualization(None);
    metered.process(&mut other[..500]);
    assert!(metered.meters[0].spectrum.is_none());
}
fn sine(hz: f64, amplitude: f64, frames: usize) -> Vec<[f64; 2]> {
    (0..frames)
        .map(|i| {
            let x = (i as f64 * std::f64::consts::TAU * hz / SAMPLE_RATE).sin() * amplitude;
            [x, x * 0.5]
        })
        .collect()
}
#[test]
fn chain_validation_rejects_unbounded_nonfinite_and_duplicate_ids() {
    let mut c = Chain::narration();
    c.validate().unwrap();
    c.effects[0].bands[0].frequency = f32::NAN;
    assert!(c.validate().is_err());
    c = Chain::narration();
    c.effects[1].id = c.effects[0].id;
    assert!(c.validate().is_err());
    c = Chain::default();
    for _ in 0..MAX_EFFECTS {
        c.add(Kind::Limiter).unwrap();
    }
    assert!(c.add(Kind::Limiter).is_err());
}
#[test]
fn bypass_and_neutral_equalizer_preserve_audio() {
    let original = sine(997., 0.2, 4800);
    let mut c = Chain::narration();
    c.bypass = true;
    let mut x = original.clone();
    Processor::new(&c).unwrap().process(&mut x);
    assert_eq!(x, original);
    let c = Chain {
        bypass: false,
        mix: 1.,
        effects: vec![Effect::new(1, Kind::Equalizer)],
    };
    Processor::new(&c).unwrap().process(&mut x);
    for (a, b) in x.iter().zip(original) {
        assert!((a[0] - b[0]).abs() < 1e-10);
    }
}
#[test]
fn filters_have_expected_response_and_block_size_does_not_change_sound() {
    let mut f = Effect::new(1, Kind::Equalizer);
    f.bands[1].frequency = 1000.;
    f.bands[1].gain = 6.;
    assert!((f.response_db(1000.) - 6.).abs() < 0.01);
    let mut hp = Effect::new(2, Kind::Equalizer);
    hp.bands = vec![Band::new(BandKind::HighPass, 80.)];
    assert!(hp.response_db(20.) < -20.);
    assert!(hp.response_db(2000.).abs() < 0.1);
    let chain = Chain {
        bypass: false,
        mix: 1.,
        effects: vec![f],
    };
    let mut a = sine(1000., 0.2, 9600);
    let mut b = a.clone();
    Processor::new(&chain).unwrap().process(&mut a);
    let mut p = Processor::new(&chain).unwrap();
    for chunk in b.chunks_mut(127) {
        p.process(chunk);
    }
    assert_eq!(a, b);
}
#[test]
fn compressor_links_channels_and_limiter_holds_sample_ceiling() {
    let mut c = Chain {
        bypass: false,
        mix: 1.,
        effects: vec![Effect::new(1, Kind::Compressor)],
    };
    let mut x = sine(600., 0.9, 48000);
    Processor::new(&c).unwrap().process(&mut x);
    assert!(x[24000..].iter().all(|s| s[0].abs() < 0.6));
    assert!(x.iter().all(|s| (s[0] * 0.5 - s[1]).abs() < 1e-9));
    c.effects = vec![Effect::new(1, Kind::Limiter)];
    let mut x = vec![[2., -2.]; 4800];
    Processor::new(&c).unwrap().process(&mut x);
    assert!(
        x.iter()
            .all(|s| s.iter().all(|v| v.abs() <= gain(-1.) + 1e-10))
    );
}
#[test]
fn every_processor_remains_finite_on_silence_and_large_signals() {
    for k in Kind::ALL {
        let c = Chain {
            bypass: false,
            mix: 1.,
            effects: vec![Effect::new(1, k)],
        };
        let mut p = Processor::new(&c).unwrap();
        let mut x = vec![[0.; 2]; 4800];
        p.process(&mut x);
        assert!(x.iter().all(|s| s == &[0.; 2]));
        let mut x = vec![[2., -2.]; 4800];
        p.process(&mut x);
        assert!(x.iter().flatten().all(|v| v.is_finite() && v.abs() <= 64.));
    }
}
#[test]
fn analysis_rejects_silence_and_proposes_measured_reversible_settings() {
    use analysis::*;
    let mut a = Analyzer::default();
    a.push(&vec![0; 48000 * 2]);
    assert!(!a.report().usable);
    let mut a = Analyzer::default();
    let x: Vec<i16> = sine(180., 0.2, 48000 * 6)
        .into_iter()
        .flatten()
        .map(|v| (v * 32767.) as i16)
        .collect();
    for chunk in x.chunks(2048) {
        a.push(chunk);
    }
    let r = a.report();
    assert!(r.usable);
    assert!((r.peak_db + 13.98).abs() < 0.1);
    let original = Chain::narration();
    let n = r.suggest(Character::Natural, 45., &original, true).unwrap();
    let bright = r.suggest(Character::Bright, 65., &original, true).unwrap();
    assert_ne!(n, bright);
    n.validate().unwrap();
    let mut old = original.clone();
    old.effects[2].enabled = false;
    old.effects[2].values[1] = 77.;
    let kept = r.suggest(Character::Natural, 45., &old, true).unwrap();
    let clarity = kept
        .effects
        .iter()
        .find(|e| e.kind == Kind::Clarity)
        .unwrap();
    assert!(!clarity.enabled);
    assert_eq!(clarity.values[1], 77.);
    assert_eq!(original, Chain::narration());
}

#[test]
fn live_parameter_bypass_and_order_updates_smooth_without_resetting_the_stream() {
    let mut chain = Chain {
        bypass: false,
        mix: 1.,
        effects: vec![Effect::new(1, Kind::Equalizer)],
    };
    let mut p = Processor::new(&chain).unwrap();
    let mut first = vec![[0.1; 2]; 1000];
    p.process(&mut first);
    chain.effects[0].values[0] = 6.;
    p.reconfigure(&chain).unwrap();
    let mut next = vec![[0.1; 2]; 1000];
    p.process(&mut next);
    assert!((next[0][0] - 0.1).abs() < 0.001);
    assert!((next[999][0] - 0.1 * gain(6.)).abs() < 1e-6);
    chain.bypass = true;
    p.reconfigure(&chain).unwrap();
    p.process(&mut next);
    assert!((next[0][0] - 0.1).abs() > 0.01);
    let mut final_block = vec![[0.1; 2]; 480];
    p.process(&mut final_block);
    assert_eq!(final_block, vec![[0.1; 2]; 480]);
}

#[test]
fn adaptive_analysis_changes_eq_by_voice_spectrum_strength_and_ignores_silence() {
    use analysis::*;
    fn voice(mud: f64, presence: f64, sharp: f64) -> Vec<i16> {
        (0..96000)
            .flat_map(|i| {
                let t = i as f64 / 48000.;
                let env = 0.55 + 0.45 * (t * 4.).sin().abs();
                let x = env
                    * (0.05 * (std::f64::consts::TAU * 960. * t).sin()
                        + mud * (std::f64::consts::TAU * 340. * t).sin()
                        + presence * (std::f64::consts::TAU * 2720. * t).sin()
                        + sharp * (std::f64::consts::TAU * 6800. * t).sin());
                [(x * 32767.) as i16; 2]
            })
            .collect()
    }
    let pcm = voice(0.25, 0.012, 0.008);
    let mut a = Analyzer::default();
    a.push(&pcm);
    let report = a.report();
    let low = report
        .suggest(Character::Natural, 10., &Chain::default(), false)
        .unwrap();
    let high = report
        .suggest(Character::Natural, 85., &Chain::default(), false)
        .unwrap();
    let eq = |c: &Chain| {
        c.effects
            .iter()
            .find(|f| f.kind == Kind::Equalizer)
            .unwrap()
            .clone()
    };
    let cut = |c: &Chain| eq(c).bands.iter().map(|b| b.gain).fold(0., f32::min);
    assert!(cut(&high) < -3.);
    assert!(cut(&high) < cut(&low) * 4.);
    assert!(
        eq(&high)
            .bands
            .iter()
            .any(|b| b.gain > 0.5 && b.frequency > 1500.)
    );
    let mut bright = Analyzer::default();
    bright.push(&voice(0.02, 0.15, 0.15));
    let bright = bright
        .report()
        .suggest(Character::Natural, 85., &Chain::default(), false)
        .unwrap();
    assert_ne!(eq(&high).bands, eq(&bright).bands);
    let mut padded = Analyzer::default();
    padded.push(&vec![0; 96000]);
    padded.push(&pcm);
    padded.push(&vec![0; 96000]);
    let padded = padded.report();
    assert!((padded.rms_db - report.rms_db).abs() < 0.3);
    assert!(eq(&high).response_db(340.) < eq(&low).response_db(340.) - 2.);
    let mut processor = Processor::new(&Chain {
        effects: vec![eq(&high)],
        ..Default::default()
    })
    .unwrap();
    let mut audio: Vec<_> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|x| [x[0] as f64 / 32768., x[1] as f64 / 32768.])
        .collect();
    processor.process(&mut audio);
    let output: Vec<_> = audio
        .into_iter()
        .flatten()
        .map(|x| (x * 32767.) as i16)
        .collect();
    let mut measured = Analyzer::default();
    measured.push(&output);
    let measured = measured.report();
    let ratio = |r: &Report| r.spectrum[5] / r.spectrum[8];
    assert!(ratio(&measured) < ratio(&report) * 0.65);
    high.validate().unwrap();
    bright.validate().unwrap();
}
