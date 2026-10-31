use super::*;

fn picker() -> Picker {
    let size = [1600, 600];
    let mut pixels = Vec::with_capacity(size[0] * size[1] * 4);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let grid = if x % 40 < 2 || y % 40 < 2 { 90 } else { 235 };
            pixels.extend_from_slice(&[grid, (120 + y / 6) as u8, (70 + x / 10) as u8, 0]);
        }
    }
    Picker {
        snapshot: Snapshot {
            origin: [-800, -100],
            size: size.map(|n| n as u32),
            dimmed: pixels.iter().map(|v| v / 2).collect(),
            pixels,
            cursor: None,
        },
        start: None,
        pointer: [-400, 200],
        selection: None,
        finished: false,
        windows: vec![],
        window_targets: None,
    }
}

fn render(p: &Picker, origin: [i32; 2], size: [i32; 2], dpi: u32) -> image::RgbaImage {
    unsafe {
        let dc = MemoryDc(CreateCompatibleDC(null_mut()));
        let mut bits = null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            dc.0,
            &bitmap_info(size.map(|n| n as u32)),
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        assert!(!bitmap.0.is_null() && !bits.is_null());
        let old = SelectObject(dc.0, bitmap.0);
        p.paint_into(dc.0, origin, size, dpi);
        GdiFlush();
        SelectObject(dc.0, old);
        let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), (size[0] * size[1] * 4) as usize);
        crop_bgra(
            bytes,
            size.map(|n| n as u32),
            Region {
                x: 0,
                y: 0,
                width: size[0] as u32,
                height: size[1] as u32,
            },
        )
        .unwrap()
    }
}

#[test]
fn lens_uses_exact_undimmed_pixels_at_center_and_clamps_only_the_screen_edges() {
    let p = picker();
    for zoom in [3, 5, 6, 10, 15] {
        for point in [[-800, -100], [799, 499], [-400, 200], [0, 0]] {
            let lens = lens_pixels(&p.snapshot, point, zoom);
            let side = SAMPLES * zoom;
            for y in 0..side {
                for x in 0..side {
                    let sx = (point[0] + 800 + x / zoom - 10).clamp(0, 1599);
                    let sy = (point[1] + 100 + y / zoom - 10).clamp(0, 599);
                    let src = ((sy * 1600 + sx) * 4) as usize;
                    let dst = ((y * side + x) * 4) as usize;
                    assert_eq!(&lens[dst..dst + 4], &p.snapshot.pixels[src..src + 4]);
                }
            }
        }
    }
}

#[test]
fn lens_stays_inside_active_monitor_and_avoids_pointer_at_edges_and_dpi_changes() {
    for size in [[320, 240], [800, 600], [3840, 2160]] {
        for dpi in [96, 120, 144, 192, 288] {
            for point in [
                [0, 0],
                [size[0] - 1, 0],
                [0, size[1] - 1],
                [size[0] - 1, size[1] - 1],
                [size[0] / 2, size[1] / 2],
            ] {
                let l = Layout::new(point, size, dpi);
                assert!(l.card.left >= 0 && l.card.top >= 0);
                assert!(l.card.right <= size[0] && l.card.bottom <= size[1]);
                assert!(
                    point[0] < l.card.left
                        || point[0] >= l.card.right
                        || point[1] < l.card.top
                        || point[1] >= l.card.bottom,
                    "{size:?} {point:?} {dpi}"
                );
            }
        }
    }
}

#[test]
fn production_picker_paints_full_guides_and_lens_before_and_during_drag_without_changing_capture() {
    let mut p = picker();
    let original = p.snapshot.pixels.clone();
    for (name, point, pressed, dpi) in [
        ("hover", [-400, 200], false, 96),
        ("drag", [-230, 320], true, 96),
        ("reverse-drag", [-500, 120], false, 96),
        ("top-left-125", [-800, -100], false, 120),
        ("bottom-right-200", [-1, 499], false, 192),
        ("other-monitor", [400, 200], false, 96),
    ] {
        assert!(p.pointer_event(point, pressed, false));
        let origin = if point[0] < 0 {
            [-800, -100]
        } else {
            [0, -100]
        };
        let size = [800, 600];
        let image = render(&p, origin, size, dpi);
        let xy = [point[0] - origin[0], point[1] - origin[1]];
        let l = Layout::new(xy, size, dpi);
        let center = [
            l.lens.left + 10 * l.zoom + l.zoom / 2,
            l.lens.top + 10 * l.zoom + l.zoom / 2,
        ];
        let src = (((point[1] + 100) * 1600 + point[0] + 800) * 4) as usize;
        assert_eq!(
            image.get_pixel(center[0] as u32, center[1] as u32).0,
            [original[src + 2], original[src + 1], original[src], 255]
        );
        // Guides reach both outer edges, including before a drag begins.
        let cyan = [0, 170, 226, 255];
        for range in [0..20, 780..800] {
            assert!(
                range
                    .clone()
                    .any(|x| image.get_pixel(x, xy[1] as u32).0 == cyan),
                "horizontal {name}"
            );
        }
        for range in [0..20, 580..600] {
            assert!(
                range
                    .clone()
                    .any(|y| image.get_pixel(xy[0] as u32, y).0 == cyan),
                "vertical {name}"
            );
        }
        if let Some(dir) = std::env::var_os("CLIPCRAFT_REGION_PREVIEW_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image.save(dir.join(format!("{name}.png"))).unwrap();
        }
    }
    assert_eq!(p.snapshot.pixels, original);
    p.pointer_event([420, 240], false, true);
    assert!(p.finished);
    let region = p.selection.unwrap();
    assert_eq!(
        p.snapshot.document(region).unwrap().render().unwrap(),
        crop_bgra(&original, p.snapshot.size, region).unwrap()
    );
}

#[test]
fn other_monitor_has_guides_without_a_duplicate_lens_and_window_capture_stays_stable() {
    let mut p = picker();
    let other = render(&p, [0, -100], [800, 600], 144);
    let expected = crop_bgra(
        &p.snapshot.dimmed,
        p.snapshot.size,
        Region {
            x: 800,
            y: 0,
            width: 800,
            height: 600,
        },
    )
    .unwrap();
    for (x, y, pixel) in other.enumerate_pixels() {
        if y != 300 && y != 301 {
            assert_eq!(pixel, expected.get_pixel(x, y));
        }
    }
    p.window_targets = Some(vec![[-700, 0, -300, 300]]);
    p.pointer = [0, 0];
    assert!(p.pointer_event([-500, 100], false, false));
    assert!(!p.pointer_event([-450, 150], false, false));
    let a = render(&p, [-800, -100], [800, 600], 96);
    p.pointer_event([-400, 200], false, false);
    assert_eq!(a, render(&p, [-800, -100], [800, 600], 96));
}
