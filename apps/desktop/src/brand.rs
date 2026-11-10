//! ClipDesigner branding. Window and in-app marks share the user-supplied vector source.
use crate::egui;
use std::sync::{Arc, LazyLock};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::sync_window_icons;

const PNG: &[u8] = include_bytes!("../assets/brand/clipdesigner-256.png");
static PIXELS: LazyLock<image::RgbaImage> = LazyLock::new(|| {
    image::load_from_memory(PNG)
        .expect("validated embedded ClipDesigner logo")
        .into_rgba8()
});

pub(super) fn icon() -> Arc<egui::IconData> {
    static ICON: LazyLock<Arc<egui::IconData>> = LazyLock::new(|| {
        Arc::new(egui::IconData {
            rgba: PIXELS.as_raw().clone(),
            width: PIXELS.width(),
            height: PIXELS.height(),
        })
    });
    Arc::clone(&ICON)
}

pub(super) fn paint(ui: &egui::Ui, rect: egui::Rect) {
    let key = egui::Id::unique("clipdesigner-brand-texture");
    let texture = ui
        .ctx()
        .data(|d| d.get_temp::<egui::TextureHandle>(key))
        .unwrap_or_else(|| {
            let texture = ui.ctx().load_texture(
                "ClipDesigner logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [PIXELS.width() as usize, PIXELS.height() as usize],
                    PIXELS.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            ui.ctx().data_mut(|d| d.insert_temp(key, texture.clone()));
            texture
        });
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        egui::Color32::WHITE,
    );
}

#[cfg(windows)]
pub(super) fn native_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    // LoadIcon returns a shared process resource; the class must not destroy it.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
            101usize as *const u16,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_icon_preserves_supplied_artwork_and_matches_the_windows_icon_directory() {
        let icon = icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        // Preserve the supplied square cyan background, without a rounded mask.
        assert_eq!(&icon.rgba[..4], &[0, 170, 226, 255]);
        assert!(icon.rgba.as_chunks::<4>().0.contains(&[0, 170, 226, 255]));
        let ico = include_bytes!("../assets/brand/clipdesigner.ico");
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes(ico[4..6].try_into().unwrap()) as usize;
        assert_eq!(count, 9);
        for i in 0..count {
            let entry = &ico[6 + i * 16..6 + (i + 1) * 16];
            let size = if entry[0] == 0 {
                256
            } else {
                u32::from(entry[0])
            };
            let len = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            let pixels = image::load_from_memory(&ico[offset..offset + len])
                .unwrap()
                .into_rgba8();
            assert_eq!(pixels.dimensions(), (size, size));
            if size == 256 {
                assert_eq!(pixels.as_raw(), &icon.rgba);
            }
        }
    }
}
