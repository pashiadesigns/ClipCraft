//! Narrow compatibility path for Windows-reserved Print Screen. No other key
//! events are retained, forwarded or suppressed by this state machine.
use capture_core::keybinds::Chord;
#[derive(Default)]
pub(super) struct PrintScreen {
    pub bindings: Vec<(i32, Chord)>,
    held: bool,
    consumed: bool,
}
impl PrintScreen {
    /// None passes through; Some(None) suppresses a repeat/release, and
    /// Some(Some(id)) dispatches exactly one configured capture.
    pub fn event(
        &mut self,
        pressed: bool,
        modifiers: u8,
        injected: bool,
        accepting: bool,
    ) -> Option<Option<i32>> {
        if injected {
            return None;
        }
        if !pressed {
            self.held = false;
            return std::mem::take(&mut self.consumed).then_some(None);
        }
        if self.held {
            return self.consumed.then_some(None);
        }
        self.held = true;
        let id = accepting
            .then(|| {
                self.bindings
                    .iter()
                    .find(|(_, chord)| chord.modifiers == modifiers)
                    .map(|(id, _)| *id)
            })
            .flatten();
        self.consumed = id.is_some();
        id.map(Some)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_screen_dispatches_once_and_passes_injected_busy_and_unassigned_chords() {
        let mut p = PrintScreen::default();
        p.bindings.push((
            7,
            Chord {
                key: 0x2c,
                modifiers: 0,
            },
        ));
        assert_eq!(p.event(true, 0, false, true), Some(Some(7)));
        assert_eq!(p.event(true, 0, false, true), Some(None));
        assert_eq!(p.event(false, 0, false, true), Some(None));
        for (modifiers, injected, accepting) in
            [(1, false, true), (0, true, true), (0, false, false)]
        {
            assert_eq!(p.event(true, modifiers, injected, accepting), None);
            assert_eq!(p.event(false, modifiers, injected, accepting), None);
        }
        assert_eq!(p.event(true, 0, false, true), Some(Some(7)));
        assert_eq!(p.event(false, 0, false, false), Some(None));
    }
}
