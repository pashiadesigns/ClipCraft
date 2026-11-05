pub(super) fn enabled() -> bool {
    #[cfg(windows)]
    {
        let mut enabled: i32 = 1;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
                0x1042,
                0,
                (&mut enabled as *mut i32).cast(),
                0,
            );
        }
        enabled != 0
    }
    #[cfg(not(windows))]
    {
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transitions_reverse_from_current_position_and_respect_reduced_motion() {
        let mut t = Tween::new(52.);
        assert_eq!(t.advance(280., 0., 0.260), 52.);
        let middle = t.advance(280., 0.130, 0.260);
        assert!(middle > 52. && middle < 280.);
        assert_eq!(t.advance(52., 0.130, 0.260), middle);
        assert_eq!(t.advance(52., 0.390, 0.260), 52.);
        assert_eq!(t.advance(280., 0.4, 0.), 280.);
    }
}

/// Reversible ease-out transition. A changed target starts at the currently
/// displayed value, so leaving/re-entering cannot jump to an old endpoint.
#[derive(Clone, Copy)]
pub(super) struct Tween {
    from: f32,
    to: f32,
    started: f64,
}
impl Tween {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            started: 0.,
        }
    }
    pub fn value(&self, now: f64, duration: f64) -> f32 {
        let t = ((now - self.started) / duration.max(0.001)).clamp(0., 1.) as f32;
        self.from + (self.to - self.from) * (1. - (1. - t).powi(3))
    }
    pub fn advance(&mut self, target: f32, now: f64, duration: f64) -> f32 {
        let current = self.value(now, duration);
        if self.to != target {
            self.from = current;
            self.to = target;
            self.started = now;
        }
        if duration <= 0. {
            self.from = target;
            self.to = target;
            return target;
        }
        self.value(now, duration)
    }
}
