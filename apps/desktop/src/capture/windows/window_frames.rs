//! Print the target window on a separate worker so occluders are never recorded
//! and a slow target cannot block the recorder's Pause/Cancel message pump.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use windows_sys::Win32::Storage::Xps::PrintWindow;

pub(super) struct WindowFrames {
    rx: Receiver<Result<Vec<u8>, String>>,
    stop: Arc<AtomicBool>,
    last: Vec<u8>,
    refreshed: Instant,
    ready: bool,
}
impl Drop for WindowFrames {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl WindowFrames {
    pub fn start(
        target: usize,
        bounds: RECT,
        origin: [i32; 2],
        source: [u32; 2],
        size: [u32; 2],
        cursor: bool,
        fps: u32,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ended = stop.clone();
        std::thread::spawn(move || {
            let run = || -> Result<(), String> {
                let native_size = [
                    (bounds.right - bounds.left).max(1) as u32,
                    (bounds.bottom - bounds.top).max(1) as u32,
                ];
                if native_size[0] as u64 * native_size[1] as u64 > capture_core::MAX_PIXELS {
                    return Err("The selected window is too large to record.".into());
                }
                let native = Frames::new(native_size)?;
                let mut output = Frames::new(size)?;
                let hwnd = target as HWND;
                while !ended.load(Ordering::Relaxed) {
                    let tick = Instant::now();
                    unsafe {
                        // PW_RENDERFULLCONTENT preserves the window's compositor content.
                        if IsWindow(hwnd) == 0 || PrintWindow(hwnd, native.memory.0, 2) == 0 {
                            return Err("Cannot capture the selected window. Restore the window or use Region recording.".into());
                        }
                        if StretchBlt(
                            output.memory.0,
                            0,
                            0,
                            size[0] as i32,
                            size[1] as i32,
                            native.memory.0,
                            origin[0] - bounds.left,
                            origin[1] - bounds.top,
                            source[0] as i32,
                            source[1] as i32,
                            SRCCOPY,
                        ) == 0
                        {
                            return Err(win_error("Cannot scale the window capture"));
                        }
                        GdiFlush();
                        if cursor && GetForegroundWindow() == hwnd {
                            output.cursor.draw(
                                std::slice::from_raw_parts_mut(
                                    output.bits,
                                    size[0] as usize * size[1] as usize * 4,
                                ),
                                origin,
                                source,
                                size,
                            );
                        }
                        let pixels = std::slice::from_raw_parts(
                            output.bits,
                            size[0] as usize * size[1] as usize * 4,
                        )
                        .to_vec();
                        if tx.send(Ok(pixels)).is_err() {
                            return Ok(());
                        }
                    }
                    std::thread::sleep(
                        Duration::from_millis(1000 / fps as u64).saturating_sub(tick.elapsed()),
                    );
                }
                Ok(())
            };
            if let Err(e) = run() {
                let _ = tx.send(Err(e));
            }
        });
        Self {
            rx,
            stop,
            last: vec![0; size[0] as usize * size[1] as usize * 4],
            refreshed: Instant::now(),
            ready: false,
        }
    }
    pub fn frame(&mut self) -> Result<Option<&[u8]>, String> {
        // No countdown is required. A pending first frame is normal, not a
        // capture failure. The recorder keeps pumping controls while waiting.
        let mut newest = None;
        let mut disconnected = false;
        loop {
            match self.rx.try_recv() {
                Ok(frame) => newest = Some(frame?),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if let Some(frame) = newest {
            self.last = frame;
            self.refreshed = Instant::now();
            self.ready = true;
        } else if disconnected || self.refreshed.elapsed() > Duration::from_secs(3) {
            return Err(
                "The selected window stopped supplying frames. Restore it and try recording again."
                    .into(),
            );
        }
        Ok(self.ready.then_some(self.last.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_window_frame_can_arrive_after_recording_opens() {
        let (tx, rx) = mpsc::channel();
        let mut frames = WindowFrames {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            last: vec![],
            refreshed: Instant::now(),
            ready: false,
        };
        assert!(frames.frame().unwrap().is_none());
        tx.send(Ok(vec![10, 20, 30, 255])).unwrap();
        assert_eq!(frames.frame().unwrap(), Some([10, 20, 30, 255].as_slice()));
        assert!(frames.frame().unwrap().is_some());
        frames.refreshed = Instant::now() - Duration::from_secs(4);
        assert!(frames.frame().is_err());
    }
    #[test]
    fn window_worker_failure_is_reported_without_publishing_blank_pixels() {
        let (tx, rx) = mpsc::channel();
        let mut frames = WindowFrames {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            last: vec![],
            refreshed: Instant::now(),
            ready: false,
        };
        tx.send(Err("Target unavailable".into())).unwrap();
        assert_eq!(frames.frame().unwrap_err(), "Target unavailable");
        drop(tx);
        assert!(frames.frame().is_err());
    }
}
