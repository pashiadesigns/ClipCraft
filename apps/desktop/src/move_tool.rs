use crate::{Editor, egui};
pub(super) struct Marquee {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub additive: bool,
}

impl Editor {
    pub(super) fn finish_marquee(&mut self) {
        let Some(m) = self.marquee.take() else { return };
        let bounds = egui::Rect::from_two_pos(m.from.into(), m.to.into());
        let document = &self.session.document;
        let ids: Vec<_> = document
            .annotations
            .iter()
            .filter(|a| {
                document.object_editable(a.id)
                    && document.group_members(a.id).iter().all(|id| {
                        let a = document.annotations.iter().find(|a| a.id == *id).unwrap();
                        let (min, max) = a.bounds();
                        document.object_editable(*id)
                            && bounds
                                .contains_rect(egui::Rect::from_min_max(min.into(), max.into()))
                    })
            })
            .map(|a| a.id)
            .collect();
        if !m.additive {
            self.select_only(None);
        }
        for id in ids {
            if !self.is_selected(id) {
                self.click_selection(Some(id), true);
            }
        }
        self.status = format!("{} objects selected.", self.selected_objects().len());
    }
    pub(super) fn paint_marquee(
        &self,
        painter: &egui::Painter,
        transform: crate::canvas_view::Transform,
    ) {
        if let Some(m) = &self.marquee {
            let rect = egui::Rect::from_two_pos(
                transform.screen_point(m.from),
                transform.screen_point(m.to),
            );
            painter.rect(
                rect,
                0.,
                egui::Color32::from_rgba_unmultiplied(40, 165, 220, 24),
                egui::Stroke::new(1., egui::Color32::from_rgb(66, 184, 231)),
                egui::StrokeKind::Inside,
            );
        }
    }
    pub(super) fn move_functions(&self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Tool Functions",
            "Move selects editable objects. Groups behave as one object; hidden or locked objects are excluded. Selection does not change document history.",
        );
        for (icon, label) in [
            ("Move", "Click an object to select it"),
            ("Selection", "Shift-click to add or remove objects"),
            ("Shape", "Drag empty space to select enclosed objects"),
            ("Move", "Drag objects or their handles"),
            ("Arrow", "Arrow keys nudge; Shift moves 10 pixels"),
            ("Text", "Double-click to switch to the object’s tool"),
        ] {
            ui.add_space(8.);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(25., 28.), egui::Sense::hover());
                crate::icons::draw_scaled(
                    ui.painter(),
                    r,
                    icon,
                    egui::Color32::from_rgb(145, 208, 232),
                    0.65,
                );
                ui.add(egui::Label::new(label).wrap());
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::tests::{editor, frame};
    use crate::{Command, Kind};
    fn fixture(ctx: &egui::Context) -> Editor {
        let mut e = editor(ctx);
        let doc: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        let template = doc
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Rectangle)
            .unwrap();
        let mut d = capture_core::Document::blank(640, 400).unwrap();
        d.annotations = (1..=3)
            .map(|id| {
                let mut a = template.clone();
                a.id = id;
                a.start = [id as f32 * 80., 90.];
                a.end = [id as f32 * 80. + 40., 130.];
                a.rotation_degrees = 0.;
                a.group = None;
                a.text = None;
                a.fill = Some([50, 180, 220, 255]);
                a
            })
            .collect();
        e.session = capture_core::Session::new(d).unwrap();
        e.tool = None;
        e.quick_controls.set_auto_switch_tool(false);
        e
    }
    fn button(p: egui::Pos2, pressed: bool, shift: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }
    }
    #[test]
    fn marquee_pointer_gestures_select_contained_objects_both_directions_without_history() {
        for reverse in [false, true] {
            let ctx = egui::Context::default();
            let mut e = fixture(&ctx);
            let t = frame(&ctx, &mut e, vec![]);
            frame(&ctx, &mut e, vec![]);
            let mut from = t.screen_point([55., 65.]);
            let mut to = t.screen_point([215., 150.]);
            if reverse {
                std::mem::swap(&mut from, &mut to);
            }
            frame(
                &ctx,
                &mut e,
                vec![egui::Event::PointerMoved(from), button(from, true, false)],
            );
            frame(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
            assert!(e.marquee.is_some());
            assert!(e.edit_drag.is_none());
            frame(&ctx, &mut e, vec![button(to, false, false)]);
            assert_eq!(
                e.selected_objects()
                    .iter()
                    .map(|a| a.id)
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            assert!(!e.session.can_undo());
            assert_eq!(e.tool, None);
            e.marquee = Some(Marquee {
                from: [235., 85.],
                to: [285., 135.],
                additive: true,
            });
            e.finish_marquee();
            assert_eq!(e.selected_objects().len(), 3);
        }
    }
    #[test]
    fn marquee_respects_group_bounds_hidden_locked_and_rotated_objects() {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        e.execute(Command::GroupObjects { ids: vec![1, 2] });
        e.marquee = Some(Marquee {
            from: [55., 65.],
            to: [135., 150.],
            additive: false,
        });
        e.finish_marquee();
        assert!(e.selected_objects().is_empty());
        e.marquee = Some(Marquee {
            from: [55., 65.],
            to: [215., 150.],
            additive: false,
        });
        e.finish_marquee();
        assert_eq!(e.selected_objects().len(), 2);
        e.execute(Command::SetObjectsLocked {
            ids: vec![2],
            locked: true,
        });
        e.marquee = Some(Marquee {
            from: [0., 0.],
            to: [220., 180.],
            additive: false,
        });
        e.finish_marquee();
        assert!(e.selected_objects().is_empty());
        e.execute(Command::SetObjectsVisible {
            ids: vec![3],
            visible: false,
        });
        e.marquee = Some(Marquee {
            from: [0., 0.],
            to: [640., 400.],
            additive: false,
        });
        e.finish_marquee();
        assert!(e.selected_objects().is_empty());
    }
    #[test]
    fn double_click_move_switches_tool_with_auto_switch_disabled() {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        let t = frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        let p = t.screen_point([100., 110.]);
        for _ in 0..2 {
            frame(
                &ctx,
                &mut e,
                vec![egui::Event::PointerMoved(p), button(p, true, false)],
            );
            frame(&ctx, &mut e, vec![button(p, false, false)]);
        }
        assert_eq!(e.tool, Some(Kind::Rectangle));
        assert_eq!(e.selected, Some(1));
        assert!(!e.session.can_undo());
    }
    #[test]
    fn group_child_double_click_drag_and_format_keep_membership_and_siblings_intact() {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        e.execute(Command::GroupObjects { ids: vec![1, 2] });
        let before = e.session.document.annotations.clone();
        let t = frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        let p = t.screen_point([100., 110.]);
        for _ in 0..2 {
            frame(
                &ctx,
                &mut e,
                vec![egui::Event::PointerMoved(p), button(p, true, false)],
            );
            frame(&ctx, &mut e, vec![button(p, false, false)]);
        }
        assert_eq!(e.selected_objects().len(), 1);
        assert_eq!(e.selected, Some(1));
        assert_eq!(e.editing_group, before[0].group);
        assert_eq!(e.tool, Some(Kind::Rectangle));
        assert!(e.text_draft.is_none());
        // A complete pointer drag moves just the child; release commits once.
        let to = p + egui::vec2(18., 20.);
        frame(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(p), button(p, true, false)],
        );
        frame(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut e, vec![button(to, false, false)]);
        assert_ne!(e.session.document.annotations[0].start, before[0].start);
        assert_eq!(e.session.document.annotations[1..], before[1..]);
        assert_eq!(e.session.document.annotations[0].group, before[0].group);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        let mut child = e.selected_annotation().unwrap();
        child.fill = Some([210, 10, 30, 255]);
        e.execute(Command::Update { annotation: child });
        assert_eq!(e.session.document.annotations[1..], before[1..]);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        // Clicking away exits child selection; a later click selects the group.
        e.select_only(None);
        e.click_selection(Some(1), false);
        assert_eq!(e.selected_objects().len(), 2);
        assert!(e.editing_group.is_none());
    }

    #[test]
    fn workspace_marquee_starts_outside_image_and_single_click_deselects_every_tool() {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        e.view.mode = crate::canvas_view::ZoomMode::Manual(0.65);
        let t = frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        let from = t.screen_point([-20., -15.]);
        assert!(t.viewport.contains(from));
        assert!(!t.image.contains(from));
        let to = t.screen_point([220., 155.]);
        frame(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(from), button(from, true, false)],
        );
        frame(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        assert!(e.marquee.is_some());
        frame(&ctx, &mut e, vec![button(to, false, false)]);
        assert_eq!(e.selected_objects().len(), 2);
        assert!(!e.session.can_undo());
        for tool in [
            None,
            Some(Kind::Arrow),
            Some(Kind::Rectangle),
            Some(Kind::Text),
            Some(Kind::Blur),
            Some(Kind::Step),
            Some(Kind::Eraser),
        ] {
            e.tool = tool;
            e.select_only(Some(1));
            frame(
                &ctx,
                &mut e,
                vec![egui::Event::PointerMoved(from), button(from, true, false)],
            );
            frame(&ctx, &mut e, vec![button(from, false, false)]);
            assert!(e.selected_objects().is_empty(), "{tool:?}");
            assert_eq!(e.tool, tool);
        }
        assert!(!e.session.can_undo());
    }
}
