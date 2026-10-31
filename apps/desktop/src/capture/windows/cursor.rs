//! Draw a frozen system cursor into offscreen DIBs; never into screen pixels.
use super::*;
use capture_core::captured_cursor::Cursor;
struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}

pub(super) fn current() -> Result<Option<Cursor>, String> {
    unsafe {
        let mut info = CURSORINFO {
            cbSize: size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if GetCursorInfo(&mut info) == 0 {
            return Err(win_error("Cannot read capture cursor"));
        }
        if info.flags & CURSOR_SHOWING == 0 || info.hCursor.is_null() {
            return Ok(None);
        }
        extract(info.hCursor, [info.ptScreenPos.x, info.ptScreenPos.y]).map(Some)
    }
}
fn extract(handle: HICON, point: [i32; 2]) -> Result<Cursor, String> {
    unsafe {
        let icon = Icon(CopyIcon(handle));
        if icon.0.is_null() {
            return Err(win_error("Cannot copy capture cursor"));
        }
        let mut info = ICONINFO::default();
        if GetIconInfo(icon.0, &mut info) == 0 {
            return Err(win_error("Cannot read cursor hotspot"));
        }
        let _mask = Bitmap(info.hbmMask);
        let _color = Bitmap(info.hbmColor);
        let mut bm = BITMAP::default();
        let color = !info.hbmColor.is_null();
        if GetObjectW(
            if color { info.hbmColor } else { info.hbmMask },
            size_of::<BITMAP>() as i32,
            (&mut bm as *mut BITMAP).cast(),
        ) == 0
        {
            return Err(win_error("Cannot measure capture cursor"));
        }
        let size = [bm.bmWidth, bm.bmHeight / if color { 1 } else { 2 }];
        if size.iter().any(|n| !(1..=512).contains(n)) {
            return Err("System cursor exceeds 512 pixels".into());
        }
        let size = size.map(|v| v as u32);
        let black = draw(icon.0, size, 0)?;
        let white = draw(icon.0, size, 255)?;
        Cursor::from_backgrounds(
            [
                point[0] - info.xHotspot as i32,
                point[1] - info.yHotspot as i32,
            ],
            &black,
            &white,
        )
        .map_err(|e| e.to_string())
    }
}
fn draw(icon: HICON, size: [u32; 2], background: u8) -> Result<image::RgbaImage, String> {
    unsafe {
        let dc = MemoryDc(CreateCompatibleDC(null_mut()));
        if dc.0.is_null() {
            return Err(win_error("Cannot create cursor surface"));
        }
        let mut bits = null_mut();
        let bitmap = Bitmap(CreateDIBSection(
            dc.0,
            &bitmap_info(size),
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        ));
        if bitmap.0.is_null() || bits.is_null() {
            return Err(win_error("Cannot allocate cursor surface"));
        }
        let pixels = std::slice::from_raw_parts_mut(
            bits.cast::<u8>(),
            size[0] as usize * size[1] as usize * 4,
        );
        pixels.fill(background);
        let previous = SelectObject(dc.0, bitmap.0);
        let ok = DrawIconEx(
            dc.0,
            0,
            0,
            icon,
            size[0] as i32,
            size[1] as i32,
            0,
            null_mut(),
            DI_NORMAL,
        );
        GdiFlush();
        SelectObject(dc.0, previous);
        if ok == 0 {
            return Err(win_error("Cannot render cursor surface"));
        }
        Ok(image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            let i = (y * size[0] + x) as usize * 4;
            image::Rgba([pixels[i + 2], pixels[i + 1], pixels[i], 255])
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn built_in_cursors_roundtrip_over_dark_and_light_synthetic_surfaces() {
        let mut fixture = Document::from_image(image::RgbaImage::from_fn(640, 400, |x, y| {
            image::Rgba(if (x / 80 + y / 80) % 2 == 0 {
                [240, 244, 248, 255]
            } else {
                [55, 85, 115, 255]
            })
        }))
        .unwrap();
        for (name, resource) in [
            ("arrow", IDC_ARROW),
            ("text", IDC_IBEAM),
            ("hand", IDC_HAND),
            ("resize", IDC_SIZENWSE),
        ] {
            let handle = unsafe { LoadCursorW(null_mut(), resource) };
            assert!(!handle.is_null());
            let cursor = extract(handle, [200, 200]).unwrap();
            let size = cursor.bitmap.size();
            let mut a = cursor
                .bitmap
                .clone()
                .annotation(fixture.annotations.len() as u64 + 1);
            a.start = [60. + fixture.annotations.len() as f32 * 140., 150.];
            a.end = [a.start[0] + 64., a.start[1] + 64.];
            fixture.annotations.push(a);
            for background in [0, 255] {
                let mut document =
                    Document::canvas(size[0], size[1], [background, background, background, 255])
                        .unwrap();
                document
                    .annotations
                    .push(cursor.bitmap.clone().annotation(1));
                document.promote_features();
                let actual = document.render().unwrap();
                let expected = draw(handle, size, background).unwrap();
                assert!(
                    actual.pixels().zip(expected.pixels()).all(|(a, b)| a
                        .0
                        .into_iter()
                        .zip(b.0)
                        .all(|(a, b)| a.abs_diff(b) <= 1)),
                    "{name} on {background}"
                );
                if let Some(path) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
                    let path = std::path::PathBuf::from(path);
                    std::fs::create_dir_all(&path).unwrap();
                    actual
                        .save(path.join(format!("{name}-{background}.png")))
                        .unwrap();
                }
            }
        }
        fixture.promote_features();
        if let Some(path) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
            let path = std::path::PathBuf::from(path);
            fixture.save(&path.join("cursor.capture.json")).unwrap();
            fixture
                .render()
                .unwrap()
                .save(path.join("cursor-export.png"))
                .unwrap();
        }
    }
}
