//! Hide tracks (AX4 request): a view-only filter that hides whole kinds of
//! timeline tracks to make navigation easier. Nothing in the project changes.
use super::*;
use crate::tips::Tip as _;
use capture_core::video::{editor::TrackKind, links::Layer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TrackType {
    Annotations,
    Instructions,
    Privacy,
    Zoom,
    Media,
    Audio,
}
impl TrackType {
    pub(crate) const ALL: [Self; 6] = [
        Self::Annotations,
        Self::Instructions,
        Self::Privacy,
        Self::Zoom,
        Self::Media,
        Self::Audio,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Annotations => "Annotation tracks",
            Self::Instructions => "Instruction tracks",
            Self::Privacy => "Privacy tracks",
            Self::Zoom => "Zoom tracks",
            Self::Media => "Media tracks",
            Self::Audio => "Audio tracks",
        }
    }
}
/// Instruction automation names its tracks; redaction-only tracks are privacy tracks.
pub(crate) fn track_kind(doc: &capture_core::Document, p: &Project, layer: Layer) -> TrackType {
    match layer {
        Layer::Zoom(_) => TrackType::Zoom,
        Layer::Media(id) => match p.track_kind(id) {
            TrackKind::Audio | TrackKind::Voiceover => TrackType::Audio,
            _ => TrackType::Media,
        },
        Layer::Annotation(track) => {
            if matches!(
                p.custom_track_name(layer),
                Some("Click highlights" | "Cursor" | "Instructions")
            ) {
                return TrackType::Instructions;
            }
            let mut kinds = p
                .objects
                .iter()
                .filter(|(_, t)| t.track == track)
                .filter_map(|(id, _)| doc.annotations.iter().find(|a| a.id == *id))
                .map(|a| a.kind)
                .peekable();
            if kinds.peek().is_some()
                && kinds.all(|k| {
                    matches!(
                        k,
                        crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate
                    )
                })
            {
                TrackType::Privacy
            } else {
                TrackType::Annotations
            }
        }
    }
}
#[cfg(test)]
#[test]
fn hidden_rows_take_no_height_and_hit_testing_skips_them() {
    use super::keyframes::{row_at, row_hidden, row_y, set_hidden_rows};
    set_hidden_rows(vec![false, true, true, false]);
    assert!(row_hidden(1) && !row_hidden(3));
    assert_eq!(row_y(0., 3, None), 39.);
    assert_eq!(row_at(0., 45., None), 3);
    assert_eq!(row_at(0., 10., None), 0);
    // Keyframe lanes under a visible row still push later rows down.
    assert_eq!(row_y(0., 3, Some((0, 2))), 39. + 50.);
    set_hidden_rows(vec![]);
    assert_eq!(row_y(0., 3, None), 117.);
    assert_eq!(row_at(0., 45., None), 1);
}
impl Editor {
    pub(super) fn hide_tracks_menu(&mut self, ui: &mut egui::Ui) {
        let count = self.video.hidden_tracks.len();
        let button = super::ui::button(
            ui,
            "Hide tracks",
            if count == 0 { "Hide" } else { "Hidden" },
            if count == 0 { 72. } else { 86. },
            count > 0,
        )
        .tip("Hide kinds of tracks to make the timeline easier to navigate");
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("hide-tracks-button"), button.rect));
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(200.);
                for kind in TrackType::ALL {
                    let mut hidden = self.video.hidden_tracks.contains(&kind);
                    if ui.checkbox(&mut hidden, kind.label()).changed() {
                        if hidden {
                            self.video.hidden_tracks.insert(kind);
                        } else {
                            self.video.hidden_tracks.remove(&kind);
                        }
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(count > 0, egui::Button::new("Show all tracks"))
                    .clicked()
                {
                    self.video.hidden_tracks.clear();
                }
            });
    }
}
