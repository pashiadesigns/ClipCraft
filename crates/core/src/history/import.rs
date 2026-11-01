//! Sequential bounded imports: one failure never discards successful items.
use super::*;
#[derive(Debug)]
pub struct Outcome {
    pub path: PathBuf,
    pub result: std::result::Result<Entry, String>,
}
impl History {
    pub fn import_image(&mut self, path: &Path) -> Result<Entry> {
        let document = Document::open_image(path)?;
        let label: String = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .filter(|c| !c.is_control())
            .take(100)
            .collect();
        self.create(
            &document,
            if label.trim().is_empty() {
                "Imported image"
            } else {
                &label
            },
            Origin::Image,
        )
    }
}
pub fn run(
    root: &Path,
    paths: &[PathBuf],
    cancelled: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(Outcome),
) {
    for path in paths.iter().take(1000) {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        // Reopen between files: each create locks and respects concurrent autosaves.
        let result = History::open(root)
            .and_then(|mut h| h.import_image(path))
            .map_err(|e| e.to_string());
        progress(Outcome {
            path: path.clone(),
            result,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_imports_keep_successes_sources_and_capacity_without_eviction() {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let good = source.path().join("Original.png");
        image::RgbaImage::from_pixel(32, 24, image::Rgba([20, 120, 200, 255]))
            .save(&good)
            .unwrap();
        let bytes = std::fs::read(&good).unwrap();
        let bad = source.path().join("Broken.png");
        std::fs::write(&bad, b"invalid").unwrap();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut results = vec![];
        run(
            root.path(),
            &[good.clone(), bad, good.clone()],
            &stop,
            |v| results.push(v),
        );
        assert!(
            results[0].result.is_ok() && results[1].result.is_err() && results[2].result.is_ok()
        );
        let mut h = History::open(root.path()).unwrap();
        assert_eq!(h.entries().len(), 2);
        let original = h.entries().to_vec();
        assert_eq!(
            h.load(&original[0].id)
                .unwrap()
                .render()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [20, 120, 200, 255]
        );
        h.set_limits(Limits {
            entries: 2,
            bytes: 1_000_000,
        })
        .unwrap();
        assert!(h.import_image(&good).is_err());
        assert_eq!(h.entries(), original);
        stop.store(true, Ordering::Relaxed);
        run(root.path(), std::slice::from_ref(&good), &stop, |_| {
            panic!("cancelled import ran")
        });
        assert_eq!(std::fs::read(good).unwrap(), bytes);
    }
}
