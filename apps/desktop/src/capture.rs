//! Region selection uses physical desktop pixels, independent of editor zoom/DPI.
use capture_core::Document;
use std::sync::mpsc::{self, Receiver};

pub struct Captured {
    pub document: Document,
    pub warning: String,
    pub recognized: Option<Result<crate::ocr::Recognized, String>>,
    pub retained: Option<capture_core::history::Entry>,
}
impl From<Document> for Captured {
    fn from(document: Document) -> Self {
        Self {
            document,
            warning: String::new(),
            recognized: None,
            retained: None,
        }
    }
}
pub type CaptureResult = Result<Option<Captured>, String>;

pub fn start(
    frame: &eframe::Frame,
    ctx: eframe::egui::Context,
    settings: capture_core::capture_options::Settings,
    history_root: Option<std::path::PathBuf>,
) -> Receiver<CaptureResult> {
    let (sender, receiver) = mpsc::channel();
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let owner = frame.window_handle().ok().and_then(|handle| {
            if let RawWindowHandle::Win32(window) = handle.as_raw() {
                Some(window.hwnd.get())
            } else {
                None
            }
        });
        std::thread::spawn(move || {
            let grab_text =
                settings.selection == capture_core::capture_options::Selection::GrabText;
            let result = owner
                .ok_or_else(|| "Cannot access the editor window for capture.".to_string())
                .and_then(|owner| windows::select_image(owner, settings, history_root))
                .map(|result| {
                    result.map(|mut captured| {
                        if grab_text {
                            captured.recognized = Some(
                                captured
                                    .document
                                    .render()
                                    .map_err(|e| e.to_string())
                                    .and_then(|pixels| crate::ocr::recognize(&pixels)),
                            );
                        }
                        captured
                    })
                });
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    #[cfg(not(windows))]
    {
        let _ = (frame, ctx, settings, history_root);
        let _ = sender.send(Err(
            "Screen capture is currently available on Windows.".into()
        ));
    }
    receiver
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(windows, test))]
impl Region {
    pub fn window_at(
        origin: [i32; 2],
        size: [u32; 2],
        point: [i32; 2],
        windows: &[[i32; 4]],
    ) -> Option<Self> {
        windows
            .iter()
            .find(|[left, top, right, bottom]| {
                point[0] >= *left && point[0] < *right && point[1] >= *top && point[1] < *bottom
            })
            .and_then(|[left, top, right, bottom]| {
                Self::from_drag(origin, size, [*left, *top], [*right, *bottom])
            })
    }
    /// Screen coordinates may be negative; output is relative to the snapshot.
    pub fn from_drag(origin: [i32; 2], size: [u32; 2], a: [i32; 2], b: [i32; 2]) -> Option<Self> {
        let local = |point: [i32; 2]| {
            [
                (i64::from(point[0]) - i64::from(origin[0])).clamp(0, i64::from(size[0])) as u32,
                (i64::from(point[1]) - i64::from(origin[1])).clamp(0, i64::from(size[1])) as u32,
            ]
        };
        let a = local(a);
        let b = local(b);
        let region = Self {
            x: a[0].min(b[0]),
            y: a[1].min(b[1]),
            width: a[0].abs_diff(b[0]),
            height: a[1].abs_diff(b[1]),
        };
        (region.width > 0 && region.height > 0).then_some(region)
    }
}

#[cfg(any(windows, test))]
fn crop_bgra(bytes: &[u8], size: [u32; 2], region: Region) -> Result<image::RgbaImage, String> {
    if region.width == 0
        || region.height == 0
        || u64::from(region.width) * u64::from(region.height) > capture_core::MAX_PIXELS
        || u64::from(region.x) + u64::from(region.width) > u64::from(size[0])
        || u64::from(region.y) + u64::from(region.height) > u64::from(size[1])
        || bytes.len() as u64 != u64::from(size[0]) * u64::from(size[1]) * 4
    {
        return Err("Capture must be a valid region of at most 25 megapixels.".into());
    }
    Ok(image::RgbaImage::from_fn(
        region.width,
        region.height,
        |x, y| {
            let offset =
                (((region.y + y) as usize * size[0] as usize) + (region.x + x) as usize) * 4;
            // GDI's unused alpha byte is not transparency. Keep captures opaque.
            image::Rgba([bytes[offset + 2], bytes[offset + 1], bytes[offset], 255])
        },
    ))
}

#[cfg(any(windows, test))]
mod delay;
#[cfg(any(windows, test))]
mod step_pipeline;
#[cfg(windows)]
mod windows;

#[cfg(any(windows, test))]
pub(super) fn repaint_selection(
    window: bool,
    before: Option<Region>,
    after: Option<Region>,
) -> bool {
    !window || before != after
}

pub fn displays() -> Result<Vec<capture_core::capture_options::Display>, String> {
    #[cfg(windows)]
    {
        windows::displays()
    }
    #[cfg(not(windows))]
    {
        Err("Display selection is available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_and_fullscreen_geometry_respects_stacking_negative_origins_and_screen_edges() {
        let origin = [-1920, -200];
        let size = [3840, 1280];
        let windows = [[-1600, -100, -800, 500], [-1800, -150, -200, 900]];
        assert_eq!(
            Region::window_at(origin, size, [-1200, 200], &windows),
            Some(Region {
                x: 320,
                y: 100,
                width: 800,
                height: 600
            })
        );
        assert_eq!(
            Region::window_at(origin, size, [-1700, 200], &windows),
            Some(Region {
                x: 120,
                y: 50,
                width: 1600,
                height: 1050
            })
        );
        assert_eq!(Region::window_at(origin, size, [100, 200], &windows), None);
        assert_eq!(
            Region::window_at(origin, size, [-1900, 0], &[[-2000, -300, 2000, 1300]]),
            Some(Region {
                x: 0,
                y: 0,
                width: 3840,
                height: 1280
            })
        );
        // Full-display capture shares the same physical clipping primitive.
        assert_eq!(
            Region::from_drag(origin, size, [-1920, -200], [0, 880]),
            Some(Region {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            })
        );
    }

    #[test]
    fn reverse_drag_on_negative_monitor_uses_physical_pixels() {
        assert_eq!(
            Region::from_drag([-1920, -200], [3840, 1280], [-1420, 500], [-1720, 100]),
            Some(Region {
                x: 200,
                y: 300,
                width: 300,
                height: 400
            })
        );
    }

    #[test]
    fn out_of_bounds_drags_clip_and_clicks_do_not_capture() {
        assert_eq!(
            Region::from_drag([0, 0], [100, 80], [-50, -50], [200, 150]),
            Some(Region {
                x: 0,
                y: 0,
                width: 100,
                height: 80
            })
        );
        assert!(Region::from_drag([0, 0], [100, 80], [10, 10], [10, 60]).is_none());
        assert!(Region::from_drag([0, 0], [100, 80], [150, 50], [160, 70]).is_none());
    }

    #[test]
    fn crops_top_down_bgra_without_reversing_rows_or_losing_opacity() {
        let bytes: Vec<u8> = (0..12).flat_map(|i| [i, i + 20, i + 40, 0]).collect();
        let cropped = crop_bgra(
            &bytes,
            [4, 3],
            Region {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(
            cropped.into_raw(),
            vec![
                45, 25, 5, 255, 46, 26, 6, 255, 49, 29, 9, 255, 50, 30, 10, 255
            ]
        );
    }

    #[test]
    fn malformed_buffers_and_excessive_regions_are_rejected() {
        assert!(
            crop_bgra(
                &[0; 4],
                [2, 2],
                Region {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1
                }
            )
            .is_err()
        );
        assert!(
            crop_bgra(
                &[0; 16],
                [2, 2],
                Region {
                    x: 2,
                    y: 0,
                    width: 1,
                    height: 1
                }
            )
            .is_err()
        );
        assert!(
            crop_bgra(
                &[],
                [10000, 10000],
                Region {
                    x: 0,
                    y: 0,
                    width: 10000,
                    height: 10000
                }
            )
            .is_err()
        );
    }
}
