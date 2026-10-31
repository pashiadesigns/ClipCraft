//! Standard Windows print dialog; printing occurs only after the user accepts it.
use std::{mem::size_of, ptr::null};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, Storage::Xps::*, UI::Controls::Dialogs::*,
};
pub(super) fn print(
    owner: isize,
    pixels: &image::RgbaImage,
    options: &capture_core::export::Options,
) -> Result<bool, String> {
    unsafe {
        let mut dialog = PRINTDLGW {
            lStructSize: size_of::<PRINTDLGW>() as u32,
            hwndOwner: owner as HWND,
            Flags: PD_RETURNDC
                | PD_NOSELECTION
                | PD_NOPAGENUMS
                | PD_USEDEVMODECOPIESANDCOLLATE
                | PD_HIDEPRINTTOFILE
                | PD_DISABLEPRINTTOFILE,
            nCopies: 1,
            ..Default::default()
        };
        let accepted = PrintDlgW(&mut dialog) != 0;
        let error = if accepted { 0 } else { CommDlgExtendedError() };
        let result = (|| {
            if !accepted {
                return if error == 0 {
                    Ok(false)
                } else {
                    Err(format!("Printer dialog failed ({error})."))
                };
            }
            let dc = dialog.hDC;
            if dc.is_null() {
                return Err("No printer device was returned.".into());
            }
            let dpi = [
                GetDeviceCaps(dc, LOGPIXELSX as i32),
                GetDeviceCaps(dc, LOGPIXELSY as i32),
            ];
            let size = [
                GetDeviceCaps(dc, HORZRES as i32),
                GetDeviceCaps(dc, VERTRES as i32),
            ];
            if dpi.iter().chain(size.iter()).any(|n| *n <= 0) {
                return Err("Printer returned invalid page dimensions.".into());
            }
            let settings = capture_core::export::PageSettings {
                size: [
                    size[0] as f64 * 72. / dpi[0] as f64,
                    size[1] as f64 * 72. / dpi[1] as f64,
                ],
                ..options.page
            };
            let pages = settings
                .pages([pixels.width(), pixels.height()])
                .map_err(|e| e.to_string())?;
            let title = "ClipCraft image\0".encode_utf16().collect::<Vec<_>>();
            let info = DOCINFOW {
                cbSize: size_of::<DOCINFOW>() as i32,
                lpszDocName: title.as_ptr(),
                lpszOutput: null(),
                ..Default::default()
            };
            if StartDocW(dc, &info) <= 0 {
                return Err("The printer could not start this job.".into());
            }
            let printed = (|| {
                SetStretchBltMode(dc, HALFTONE);
                SetBrushOrgEx(dc, 0, 0, std::ptr::null_mut());
                for page in pages {
                    let [x, y, w, h] = page.source;
                    let rgb = capture_core::export::opaque(
                        &image::imageops::crop_imm(pixels, x, y, w, h).to_image(),
                        options.background,
                    );
                    let bgra: Vec<u8> =
                        rgb.pixels().flat_map(|p| [p[2], p[1], p[0], 255]).collect();
                    let bitmap = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: w as i32,
                            biHeight: -(h as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    if StartPage(dc) <= 0 {
                        return Err("The printer could not start a page.".into());
                    }
                    let [x, y, w, h] = page.destination;
                    let result = StretchDIBits(
                        dc,
                        (x * dpi[0] as f64 / 72.).round() as i32,
                        (y * dpi[1] as f64 / 72.).round() as i32,
                        (w * dpi[0] as f64 / 72.).round() as i32,
                        (h * dpi[1] as f64 / 72.).round() as i32,
                        0,
                        0,
                        rgb.width() as i32,
                        rgb.height() as i32,
                        bgra.as_ptr().cast(),
                        &bitmap,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                    if result == 0 || result == -1 || EndPage(dc) <= 0 {
                        return Err("The printer could not render a page.".into());
                    }
                }
                if EndDoc(dc) <= 0 {
                    return Err("The printer could not complete this job.".into());
                }
                Ok(true)
            })();
            if printed.is_err() {
                AbortDoc(dc);
            }
            printed
        })();
        if !dialog.hDC.is_null() {
            DeleteDC(dialog.hDC);
        }
        if !dialog.hDevMode.is_null() {
            GlobalFree(dialog.hDevMode);
        }
        if !dialog.hDevNames.is_null() {
            GlobalFree(dialog.hDevNames);
        }
        result
    }
}
