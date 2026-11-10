//! Older DWM implementations can reject exclusion for per-pixel layered windows
//! with ERROR_NOT_ENOUGH_MEMORY. Use the same pixels in a regular shaped window
//! in that case, and still require exclusion before any recorder is shown.
use super::*;
use image::RgbaImage;

pub(super) fn layered(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0 }
}

pub(super) fn exclude(hwnd: HWND, name: &str) -> Result<(), String> {
    exclude_with(hwnd, name, |window| unsafe {
        SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE) != 0
    })
}

fn exclude_with(hwnd: HWND, name: &str, mut apply: impl FnMut(HWND) -> bool) -> Result<(), String> {
    if apply(hwnd) {
        return Ok(());
    }
    let original_error = std::io::Error::last_os_error();
    unsafe {
        let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if extended as u32 & WS_EX_LAYERED != 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended & !(WS_EX_LAYERED as isize));
            // Buttons retain their input/accessibility HWNDs; the parent owns all
            // their pixels, including in the regular GDI presentation path.
            SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_CLIPCHILDREN as isize));
            if !layered(hwnd) && apply(hwnd) {
                return Ok(());
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended);
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        }
    }
    Err(format!(
        "Windows could not keep {name} out of the capture. Update Windows and the graphics driver, then retry. Capture was not started. Details: {original_error}"
    ))
}

// The input is the existing premultiplied controller/preview surface. Shallow
// translucent shadows are omitted in compatibility mode, not painted black.
fn opaque_pixels(image: &RgbaImage) -> Vec<u8> {
    let mut pixels = image.as_raw().clone();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        let straight =
            |value: u8| ((u32::from(value) * 255 + alpha / 2) / alpha.max(1)).min(255) as u8;
        *pixel = [
            straight(pixel[2]),
            straight(pixel[1]),
            straight(pixel[0]),
            255,
        ];
    }
    pixels
}

fn mask_runs(image: &RgbaImage) -> Vec<RECT> {
    let mut runs = Vec::new();
    for (y, row) in image.rows().enumerate() {
        let mut start = None;
        for (x, pixel) in row.enumerate() {
            match (start, pixel[3] >= 128) {
                (None, true) => start = Some(x as i32),
                (Some(left), false) => {
                    runs.push(RECT {
                        left,
                        top: y as i32,
                        right: x as i32,
                        bottom: y as i32 + 1,
                    });
                    start = None;
                }
                _ => (),
            }
        }
        if let Some(left) = start {
            runs.push(RECT {
                left,
                top: y as i32,
                right: image.width() as i32,
                bottom: y as i32 + 1,
            });
        }
    }
    runs
}

fn mask(image: &RgbaImage) -> HRGN {
    let rects = mask_runs(image);
    // RGNDATAHEADER and RECT both have u32 alignment. Keep backing storage alive
    // for the whole ExtCreateRegion call; Windows copies it into its own region.
    let bytes = size_of::<RGNDATAHEADER>() + rects.len() * size_of::<RECT>();
    let mut data = vec![0u32; bytes / size_of::<u32>()];
    unsafe {
        let header = data.as_mut_ptr().cast::<RGNDATAHEADER>();
        *header = RGNDATAHEADER {
            dwSize: size_of::<RGNDATAHEADER>() as u32,
            iType: RDH_RECTANGLES,
            nCount: rects.len() as u32,
            nRgnSize: (rects.len() * size_of::<RECT>()) as u32,
            rcBound: RECT {
                left: 0,
                top: 0,
                right: image.width() as i32,
                bottom: image.height() as i32,
            },
        };
        std::ptr::copy_nonoverlapping(rects.as_ptr(), header.add(1).cast::<RECT>(), rects.len());
        ExtCreateRegion(null(), bytes as u32, header.cast())
    }
}

pub(super) fn present(hwnd: HWND, image: &RgbaImage) {
    unsafe {
        let region = mask(image);
        if region.is_null() {
            return;
        }
        let prior = Bitmap(CreateRectRgn(0, 0, 0, 0));
        if !prior.0.is_null()
            && GetWindowRgn(hwnd, prior.0) != ERROR
            && EqualRgn(prior.0, region) != 0
        {
            DeleteObject(region);
        } else if SetWindowRgn(hwnd, region, 0) == 0 {
            DeleteObject(region);
            return;
        }
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        if rect.right != image.width() as i32 || rect.bottom != image.height() as i32 {
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                image.width() as i32,
                image.height() as i32,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let dc = GetDC(hwnd);
        if !dc.is_null() {
            let pixels = opaque_pixels(image);
            SetDIBitsToDevice(
                dc,
                0,
                0,
                image.width(),
                image.height(),
                0,
                0,
                0,
                image.height(),
                pixels.as_ptr().cast(),
                &bitmap_info([image.width(), image.height()]),
                DIB_RGB_COLORS,
            );
            ReleaseDC(hwnd, dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestWindow(HWND);
    impl TestWindow {
        fn new() -> Self {
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("ClipCraft exclusion regression"),
                    WS_POPUP | WS_CLIPCHILDREN,
                    0,
                    0,
                    80,
                    40,
                    null_mut(),
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                )
            };
            assert!(!hwnd.is_null());
            Self(hwnd)
        }
    }
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    #[test]
    fn exclusion_failure_retries_regular_surface_without_showing_controls() {
        let window = TestWindow::new();
        let mut calls = 0;
        exclude_with(window.0, "recorder", |hwnd| {
            calls += 1;
            assert_eq!(unsafe { IsWindowVisible(hwnd) }, 0);
            if calls == 1 {
                assert!(layered(hwnd));
                unsafe {
                    SetLastError(ERROR_NOT_ENOUGH_MEMORY);
                }
                false
            } else {
                assert!(!layered(hwnd));
                assert_eq!(
                    unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CLIPCHILDREN },
                    0
                );
                true
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert!(!layered(window.0));
    }
    #[test]
    fn failed_fallback_restores_style_and_does_not_silently_record_controls() {
        let window = TestWindow::new();
        let result = exclude_with(window.0, "recorder", |_| {
            unsafe {
                SetLastError(ERROR_NOT_ENOUGH_MEMORY);
            }
            false
        });
        assert!(result.unwrap_err().contains("Capture was not started"));
        assert!(layered(window.0));
        assert_eq!(unsafe { IsWindowVisible(window.0) }, 0);
        assert_ne!(
            unsafe { GetWindowLongPtrW(window.0, GWL_STYLE) as u32 & WS_CLIPCHILDREN },
            0
        );
    }
    #[test]
    fn supported_surface_keeps_original_composition() {
        let window = TestWindow::new();
        let mut calls = 0;
        exclude_with(window.0, "recorder", |_| {
            calls += 1;
            true
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(layered(window.0));
    }
    #[test]
    fn compatibility_surface_preserves_shape_and_straight_colors() {
        let mut image = RgbaImage::new(4, 2);
        image.put_pixel(1, 0, image::Rgba([64, 32, 16, 128]));
        image.put_pixel(2, 0, image::Rgba([0, 160, 240, 255]));
        image.put_pixel(3, 1, image::Rgba([10, 10, 10, 50]));
        let pixels = opaque_pixels(&image);
        assert_eq!(&pixels[4..8], &[32, 64, 128, 255]);
        assert_eq!(&pixels[8..12], &[240, 160, 0, 255]);
        let region = Bitmap(mask(&image));
        assert!(!region.0.is_null());
        unsafe {
            assert_ne!(PtInRegion(region.0, 1, 0), 0);
            assert_ne!(PtInRegion(region.0, 2, 0), 0);
            assert_eq!(PtInRegion(region.0, 0, 0), 0);
            assert_eq!(PtInRegion(region.0, 3, 1), 0);
        }
    }

    #[test]
    fn compatible_recorder_pixels_and_hit_regions_keep_the_approved_controls() {
        use super::super::step_recorder::visual::{Caption, render_capture};
        for (name, caption, center) in [
            ("steps", Caption::Steps, 184),
            (
                "scroll",
                Caption::Scrolling {
                    automatic: true,
                    running: false,
                    countdown: 0,
                },
                184,
            ),
            (
                "video",
                Caption::Video {
                    seconds: 1,
                    countdown: 0,
                    steps: None,
                    webcam: false,
                },
                132,
            ),
        ] {
            for expanded in [0., 1.] {
                let image = render_capture(96, expanded, 3, false, 0, 0., caption).unwrap();
                let pixels = opaque_pixels(&image);
                for (original, compatible) in image
                    .as_raw()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(pixels.as_chunks::<4>().0)
                {
                    if original[3] == 255 {
                        assert_eq!(compatible, &[original[2], original[1], original[0], 255]);
                    }
                }
                let region = Bitmap(mask(&image));
                assert_ne!(unsafe { PtInRegion(region.0, center, 42) }, 0, "{name}");
                assert_eq!(unsafe { PtInRegion(region.0, 0, 0) }, 0, "{name}");
                if let Ok(folder) = std::env::var("CLIPCRAFT_COMPAT_EVIDENCE") {
                    let folder = std::path::PathBuf::from(folder);
                    std::fs::create_dir_all(&folder).unwrap();
                    let mut compatible = image.clone();
                    for ((src, dst), bgra) in image
                        .pixels()
                        .zip(compatible.pixels_mut())
                        .zip(pixels.as_chunks::<4>().0)
                    {
                        dst.0 = [
                            bgra[2],
                            bgra[1],
                            bgra[0],
                            if src[3] >= 128 { 255 } else { 0 },
                        ];
                    }
                    compatible
                        .save(folder.join(format!("{name}-{expanded}.png")))
                        .unwrap();
                }
            }
        }
    }

    #[test]
    #[ignore = "Requires a Windows desktop with DWM capture exclusion available"]
    fn native_regular_fallback_retains_exclusion_after_presenting_and_resizing() {
        let window = TestWindow::new();
        let mut first = true;
        exclude_with(window.0, "recorder", |hwnd| {
            if std::mem::take(&mut first) {
                unsafe {
                    SetLastError(ERROR_NOT_ENOUGH_MEMORY);
                }
                false
            } else {
                unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) != 0 }
            }
        })
        .unwrap();
        for size in [[80, 40], [160, 100], [32, 24]] {
            let image = RgbaImage::from_pixel(size[0], size[1], image::Rgba([30, 60, 90, 255]));
            present(window.0, &image);
            let mut affinity = 0;
            assert_ne!(
                unsafe { GetWindowDisplayAffinity(window.0, &mut affinity) },
                0
            );
            assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE);
            assert!(!layered(window.0));
            assert_eq!(unsafe { IsWindowVisible(window.0) }, 0);
            let mut rect = RECT::default();
            unsafe {
                GetClientRect(window.0, &mut rect);
            }
            assert_eq!([rect.right as u32, rect.bottom as u32], size);
        }
    }
}
