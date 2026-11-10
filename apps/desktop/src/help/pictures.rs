//! The Help pictures: real renders of ClipCraft with a made-up sample project,
//! produced during qualification by `scripts/render-help-pictures.py`.
use std::collections::HashMap;

const PACK: &[u8] = include_bytes!("../../assets/help/pictures.pack");

fn index(pack: &'static [u8]) -> Option<HashMap<&'static str, &'static [u8]>> {
    let mut out = HashMap::new();
    let count = u32::from_le_bytes(pack.get(..4)?.try_into().ok()?) as usize;
    let mut at = 4;
    for _ in 0..count {
        let name = u16::from_le_bytes(pack.get(at..at + 2)?.try_into().ok()?) as usize;
        let data = u32::from_le_bytes(pack.get(at + 2..at + 6)?.try_into().ok()?) as usize;
        at += 6;
        let key = std::str::from_utf8(pack.get(at..at + name)?).ok()?;
        at += name;
        out.insert(key, pack.get(at..at + data)?);
        at += data;
    }
    Some(out)
}
/// The encoded picture for a key.
pub(super) fn find(key: &str) -> Option<&'static [u8]> {
    static INDEX: std::sync::OnceLock<HashMap<&'static str, &'static [u8]>> =
        std::sync::OnceLock::new();
    INDEX
        .get_or_init(|| index(PACK).unwrap_or_default())
        .get(key)
        .copied()
}
/// A decoded picture for a key.
pub(super) fn decode(key: &str) -> Option<image::RgbaImage> {
    Some(image::load_from_memory(find(key)?).ok()?.to_rgba8())
}
