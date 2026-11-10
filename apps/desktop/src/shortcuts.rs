//! Destructive document actions must not match a more specific chord that was
//! deliberately left unhandled while a text field or modal owns keyboard input.
use eframe::egui::{Event, InputState, Key, Modifiers};

/// Resolve stacking keys only while the canvas owns input. Returning a shared
/// command keeps keyboard and menu ordering, history and boundaries identical.
pub fn arrange_command(
    input: &mut InputState,
    document: &capture_core::Document,
    selected: Option<u64>,
    canvas_owns_input: bool,
) -> Option<capture_core::Command> {
    if !canvas_owns_input {
        return None;
    }
    let id = selected?;
    for (key, order) in [
        (Key::F, capture_core::StackOrder::BringForward),
        (Key::B, capture_core::StackOrder::SendBackward),
    ] {
        if consume_exact(input, Modifiers::COMMAND | Modifiers::SHIFT, key) {
            return document
                .can_arrange(id, order)
                .then_some(capture_core::Command::Arrange { id, order });
        }
    }
    None
}

pub fn consume_exact(input: &mut InputState, modifiers: Modifiers, key: Key) -> bool {
    let mut pressed = false;
    input.events.retain(|event| {
        if let Event::Key {
            key: event_key,
            modifiers: event_modifiers,
            pressed: true,
            repeat,
            ..
        } = event
            && *event_key == key
            && event_modifiers.matches_exact(modifiers)
        {
            pressed |= !repeat;
            false
        } else {
            true
        }
    });
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(key: Key, modifiers: Modifiers, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn unhandled_clipboard_chord_cannot_open_a_blank_canvas() {
        let mut input = InputState::default();
        input.events.push(event(
            Key::N,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            false,
        ));
        assert!(!consume_exact(&mut input, Modifiers::COMMAND, Key::N));
        assert!(consume_exact(
            &mut input,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::N
        ));
        assert!(!consume_exact(
            &mut input,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::N
        ));
    }

    #[test]
    fn copy_and_capture_chords_are_distinct_and_repeats_do_not_open_dialogs() {
        let mut input = InputState::default();
        input.events.push(event(
            Key::C,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            false,
        ));
        assert!(!consume_exact(
            &mut input,
            Modifiers::COMMAND | Modifiers::ALT,
            Key::C
        ));
        assert!(consume_exact(
            &mut input,
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::C
        ));
        input
            .events
            .push(event(Key::N, Modifiers::CTRL | Modifiers::COMMAND, true));
        assert!(!consume_exact(&mut input, Modifiers::COMMAND, Key::N));
        input
            .events
            .push(event(Key::N, Modifiers::CTRL | Modifiers::COMMAND, false));
        assert!(consume_exact(&mut input, Modifiers::COMMAND, Key::N));
    }

    fn stack_fixture() -> capture_core::Session {
        use capture_core::{Annotation, Document, Kind, Session};
        let mut document = Document::blank(40, 30).unwrap();
        document.annotations = [1, 2, 3]
            .into_iter()
            .map(|id| Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id,
                kind: Kind::Rectangle,
                start: [5.0, 5.0],
                end: [30.0, 20.0],
                color: [id as u8 * 60, 0, 0, 255],
                width: 3,
                text: None,
                corner_radius: 0.0,
                fill: None,
                points: Vec::new(),
                callout: None,
            })
            .collect();
        Session::new(document).unwrap()
    }

    #[test]
    fn stacking_key_events_change_pixels_and_preserve_boundary_history() {
        use capture_core::Command;
        let mut session = stack_fixture();
        let original = session.document.render().unwrap();
        let modifiers = Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT;
        let mut input = InputState::default();
        input.events.push(event(Key::F, modifiers, false));
        let command = arrange_command(&mut input, &session.document, Some(2), true).unwrap();
        session.execute(command).unwrap();
        assert_eq!(session.document.hit_test([5.0, 12.0], 0.0), Some(2));
        assert_eq!(
            session.document.render().unwrap().get_pixel(5, 12).0,
            [120, 0, 0, 255]
        );
        assert!(input.events.is_empty());
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), original);
        assert!(!session.is_modified());
        assert!(session.can_redo());
        for (key, id) in [(Key::F, 3), (Key::B, 1)] {
            input.events.push(event(key, modifiers, false));
            assert!(arrange_command(&mut input, &session.document, Some(id), true).is_none());
            assert!(input.events.is_empty());
            assert!(session.can_redo());
            assert!(!session.can_undo());
        }
        input.events.push(event(Key::B, modifiers, false));
        session
            .execute(arrange_command(&mut input, &session.document, Some(3), true).unwrap())
            .unwrap();
        assert_eq!(session.document.hit_test([5.0, 12.0], 0.0), Some(2));
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), original);
    }

    #[test]
    fn stacking_shortcuts_respect_input_ownership_modifiers_and_repeats() {
        let session = stack_fixture();
        let modifiers = Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT;
        for key in [Key::F, Key::B] {
            let mut input = InputState::default();
            input.events.push(event(key, modifiers, false));
            assert!(arrange_command(&mut input, &session.document, Some(2), false).is_none());
            assert_eq!(
                input.events.len(),
                1,
                "text/modal input must remain untouched"
            );
            assert!(arrange_command(&mut input, &session.document, None, true).is_none());
            assert_eq!(input.events.len(), 1);
            input.events.clear();
            for wrong in [
                Modifiers::NONE,
                Modifiers::CTRL | Modifiers::COMMAND,
                modifiers | Modifiers::ALT,
            ] {
                input.events.push(event(key, wrong, false));
            }
            assert!(arrange_command(&mut input, &session.document, Some(2), true).is_none());
            assert_eq!(input.events.len(), 3);
            input.events.clear();
            input.events.push(event(key, modifiers, true));
            assert!(arrange_command(&mut input, &session.document, Some(2), true).is_none());
            assert!(input.events.is_empty());
        }
    }
}
