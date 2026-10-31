//! Local installed font catalog. Projects store stable family IDs, never paths
//! or redistributed font files. Missing families use the bundled fallback.
use super::TextFont;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};
struct Face {
    path: PathBuf,
    index: u32,
    bold: bool,
    italic: bool,
    bytes: OnceLock<Option<Vec<u8>>>,
    font: OnceLock<Option<ab_glyph::FontRef<'static>>>,
}
struct Family {
    id: u64,
    name: String,
    faces: Vec<Face>,
}
fn id(name: &str) -> u64 {
    name.to_lowercase()
        .bytes()
        .fold(14695981039346656037, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(1099511628211)
        })
}
fn catalog() -> &'static [Family] {
    static CATALOG: OnceLock<Vec<Family>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut roots = vec![];
        if cfg!(windows) {
            if let Some(w) = std::env::var_os("WINDIR") {
                roots.push(PathBuf::from(w).join("Fonts"));
            }
            if let Some(l) = std::env::var_os("LOCALAPPDATA") {
                roots.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
            }
        }
        let mut families = BTreeMap::<String, Vec<Face>>::new();
        for root in roots {
            for entry in std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .take(4096)
            {
                let path = entry.path();
                if !path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                    ["ttf", "otf", "ttc"].contains(&s.to_ascii_lowercase().as_str())
                }) {
                    continue;
                }
                if entry.metadata().map_or(true, |m| m.len() > 32_000_000) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                for index in 0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1).min(64) {
                    let Ok(face) = ttf_parser::Face::parse(&bytes, index) else {
                        continue;
                    };
                    let name = [16, 1].into_iter().find_map(|id| {
                        face.names()
                            .into_iter()
                            .find_map(|n| (n.name_id == id).then(|| n.to_string()).flatten())
                    });
                    let Some(name) = name.filter(|n| !n.is_empty() && n.len() <= 256) else {
                        continue;
                    };
                    families.entry(name).or_default().push(Face {
                        path: path.clone(),
                        index,
                        bold: face.is_bold(),
                        italic: face.is_italic(),
                        bytes: OnceLock::new(),
                        font: OnceLock::new(),
                    });
                }
            }
        }
        families
            .into_iter()
            .map(|(name, faces)| Family {
                id: id(&name),
                name,
                faces,
            })
            .collect()
    })
}
pub fn choices() -> impl Iterator<Item = TextFont> {
    catalog().iter().map(|f| TextFont::System(f.id))
}
pub fn label(id: u64) -> Option<&'static str> {
    catalog()
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.name.as_str())
}
fn selected(id: u64, bold: bool, italic: bool) -> Option<&'static Face> {
    catalog()
        .iter()
        .find(|f| f.id == id)?
        .faces
        .iter()
        .min_by_key(|f| u8::from(f.bold != bold) + u8::from(f.italic != italic))
}
pub fn bytes(id: u64, bold: bool, italic: bool) -> Option<&'static [u8]> {
    let f = selected(id, bold, italic)?;
    f.bytes
        .get_or_init(|| {
            if std::fs::metadata(&f.path).ok()?.len() > 32_000_000 {
                return None;
            }
            std::fs::read(&f.path).ok()
        })
        .as_deref()
}
pub fn index(id: u64, bold: bool, italic: bool) -> u32 {
    selected(id, bold, italic)
        .filter(|_| bytes(id, bold, italic).is_some())
        .map_or(0, |f| f.index)
}
pub fn face(id: u64, bold: bool, italic: bool) -> Option<&'static ab_glyph::FontRef<'static>> {
    let f = selected(id, bold, italic)?;
    f.font
        .get_or_init(|| {
            ab_glyph::FontRef::try_from_slice_and_index(bytes(id, bold, italic)?, f.index).ok()
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_font_render_save_reopen_and_undo_agree() {
        let Some(font) = choices().find(|f| f.label() == "Arial") else {
            return;
        };
        let mut a:crate::Annotation=serde_json::from_str(r#"{"id":1,"kind":"text","start":[10,10],"end":[400,140],"color":[30,70,140,255],"width":1,"text":{"content":"Installed font Aa 0123","font":"ubuntu_light","size_points":28}}"#).unwrap();
        let mut session = crate::Session::new(crate::Document::blank(420, 160).unwrap()).unwrap();
        session
            .execute(crate::Command::Add {
                annotation: a.clone(),
            })
            .unwrap();
        let bundled = session.document.render().unwrap();
        a.text.as_mut().unwrap().font = font;
        session
            .execute(crate::Command::Update {
                annotation: a.clone(),
            })
            .unwrap();
        assert_eq!(session.document.version, 21);
        let actual = session.document.render().unwrap();
        assert_ne!(actual, bundled);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("font.capture.json");
        session.save(&path).unwrap();
        assert_eq!(
            crate::Document::load(&path).unwrap().render().unwrap(),
            actual
        );
        session.execute(crate::Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), bundled);
        session.execute(crate::Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), actual);
        let mut old = session.document.clone();
        old.version = 20;
        assert!(old.validate().is_err());
    }
    #[test]
    fn installed_family_ids_and_fallback_are_stable() {
        assert_eq!(id("Arial"), id("ARIAL"));
        assert_eq!(
            TextFont::System(u64::MAX).bytes(false, false),
            TextFont::UbuntuLight.bytes(false, false)
        );
        let values = choices().collect::<Vec<_>>();
        for f in values.iter().take(12) {
            let TextFont::System(id) = f else {
                unreachable!()
            };
            assert!(label(*id).is_some());
            assert!(!f.bytes(false, false).is_empty());
            let encoded = serde_json::to_string(f).unwrap();
            assert_eq!(*f, serde_json::from_str::<TextFont>(&encoded).unwrap());
        }
    }
}
