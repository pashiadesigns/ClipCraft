use super::*;
use capture_core::video::effects::{Band, BandKind, Effect};
pub(super) fn choices(kind: Kind, id: u64) -> Vec<(&'static str, Effect)> {
    let presets: &[(&str, [f32; 4])] = match kind {
        Kind::Equalizer => &[
            ("Natural speech", [0.; 4]),
            ("Reduce muddiness", [0.; 4]),
            ("Speech focus", [0.; 4]),
        ],
        Kind::Compressor => &[
            ("Gentle narration", [-20., 2., 20., 150.]),
            ("Steady tutorial", [-24., 3., 12., 120.]),
            ("Tame loud peaks", [-16., 5., 4., 80.]),
        ],
        Kind::Clarity => &[
            ("Natural presence", [20., 30., 10., 75.]),
            ("Warm voice", [55., 20., 10., 70.]),
            ("Crisp speech", [10., 55., 35., 80.]),
        ],
        Kind::DeEsser => &[
            ("Gentle consonants", [6500., -24., 3., 40.]),
            ("Lower sibilance", [4500., -28., 5., 60.]),
            ("Bright sibilance", [8500., -28., 6., 65.]),
        ],
        Kind::Leveler => &[
            ("Natural phrases", [-24., 4., 25., 50.]),
            ("Steady narration", [-22., 6., 40., 70.]),
            ("Quiet voice", [-20., 9., 55., 75.]),
        ],
        Kind::Limiter => &[
            ("Safe headroom", [-1., 80., 0., 0.]),
            ("Extra headroom", [-3., 100., 0., 0.]),
            ("Gentle lift", [-1., 100., 3., 0.]),
        ],
    };
    presets
        .iter()
        .enumerate()
        .map(|(i, (name, values))| {
            let mut e = Effect::new(id, kind);
            e.values = *values;
            if kind == Kind::Equalizer {
                e.bands = vec![Band::new(
                    BandKind::HighPass,
                    if i == 0 { 60. } else { 80. },
                )];
                if i > 0 {
                    let mut b = Band::bell(280., 0.9);
                    b.gain = -2.5;
                    e.bands.push(b);
                }
                if i == 2 {
                    let mut b = Band::bell(3000., 0.8);
                    b.gain = 2.;
                    e.bands.push(b);
                }
            }
            (*name, e)
        })
        .collect()
}
pub(super) fn help(kind: Kind, index: usize) -> &'static str {
    match kind {
        Kind::Compressor => [
            "Level where compression begins. Drag the gold line while listening.",
            "How strongly louder sounds are reduced.",
            "How quickly loud peaks are caught. Slower keeps more punch.",
            "How quickly the voice returns to normal gain.",
        ][index.min(3)],
        Kind::Clarity => [
            "Low-mid warmth. Reduce if the voice sounds boomy.",
            "Speech definition. Raise gently if words feel distant.",
            "High-frequency brightness. Too much can emphasize hiss.",
            "Blend processed clarity with the original voice.",
        ][index.min(3)],
        Kind::DeEsser => [
            "Center of harsh S and SH sounds. Lower for deeper voices.",
            "Sensitivity threshold for the detected consonant band.",
            "Maximum reduction of that band; the rest of the voice stays intact.",
            "How firmly sharp consonants above the threshold are softened.",
        ][index.min(3)],
        Kind::Leveler => [
            "Desired speech RMS level, not final mix LUFS.",
            "Maximum upward or downward gain correction.",
            "How quickly the level follows changing phrases.",
            "Amount of automatic leveling; lower preserves more dynamics.",
        ][index.min(3)],
        Kind::Limiter => [
            "Maximum sample peak. Leave headroom for encoding.",
            "How quickly gain recovers after a loud peak.",
            "Gain before limiting. Raise only if more level is needed.",
        ][index.min(2)],
        _ => "Shape the voice while listening. Original audio is preserved.",
    }
}
