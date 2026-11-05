use crate::{Annotation, Command, Document, Editor, canvas_view::Transform, egui};
use capture_core::{canvas_expansion::MovePlan, snapping::SnapResult};

pub(super) fn move_with_candidates(
    document: &Document,
    objects: &[Annotation],
    delta: [f32; 2],
    tolerance: Option<f32>,
    duplicate: bool,
    candidates: Option<(&[u64], usize)>,
) -> capture_core::Result<SnapResult> {
    let result = if let Some(tolerance) = tolerance {
        let mut visible: Vec<_> = document
            .annotations
            .iter()
            .filter(|a| document.object_visible(a.id) && !objects.iter().any(|o| o.id == a.id))
            .filter(|a| {
                document.video.as_ref().is_none_or(|p| {
                    p.objects.get(&a.id).is_some_and(|t| {
                        t.active(p.cursor)
                            && !p.layer_hidden(capture_core::video::links::Layer::Annotation(
                                t.track,
                            ))
                    })
                })
            })
            .collect();
        if let Some((recent, limit)) = candidates {
            visible.sort_by_key(|a| {
                (
                    recent
                        .iter()
                        .rposition(|id| *id == a.id)
                        .map(|n| n + 1)
                        .unwrap_or(0),
                    a.id,
                )
            });
            visible.reverse();
            visible.truncate(limit);
        }
        let visible: Vec<_> = visible.into_iter().cloned().collect();
        capture_core::snapping::translated_expanding(objects, &visible, delta, tolerance)?
    } else {
        SnapResult {
            annotations: capture_core::canvas_expansion::translated(objects, delta)?,
            guides: [None; 2],
        }
    };
    document.plan_expanding_move(&result.annotations, duplicate)?;
    Ok(result)
}

impl Editor {
    pub(super) fn expanding_move_plan(&self) -> Option<MovePlan> {
        if let Some(drag) = &self.group_drag {
            self.session
                .document
                .plan_expanding_move(&drag.preview, drag.duplicate)
                .ok()
        } else if let Some(drag) = self.edit_drag.as_ref().filter(|d| d.handle.is_none()) {
            self.session
                .document
                .plan_expanding_move(std::slice::from_ref(&drag.preview), false)
                .ok()
        } else if let Some(step) = &self.step_placement {
            self.session
                .document
                .plan_expanding_move(std::slice::from_ref(step), true)
                .ok()
        } else {
            None
        }
    }
    pub(super) fn expansion_transform(&self, original: Transform) -> Transform {
        let Some(plan) = self.expanding_move_plan() else {
            return original;
        };
        Transform {
            image: egui::Rect::from_min_size(
                original.image.min
                    - egui::vec2(plan.offset[0] as f32, plan.offset[1] as f32) * original.scale,
                egui::vec2(plan.dimensions[0] as f32, plan.dimensions[1] as f32) * original.scale,
            ),
            ..original
        }
    }
    pub(super) fn commit_expanding_move(
        &mut self,
        annotations: Vec<Annotation>,
        duplicate: bool,
        transform: Transform,
    ) {
        let plan = match self
            .session
            .document
            .plan_expanding_move(&annotations, duplicate)
        {
            Ok(plan) => plan,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let changed_size = plan.dimensions
            != [self.session.document.width, self.session.document.height]
            || plan.offset != [0, 0];
        self.execute(Command::MoveAndExpand {
            annotations,
            duplicate,
        });
        if changed_size && self.session.document.annotations == plan.annotations {
            self.view
                .retain_expanded_position(transform, plan.offset, plan.dimensions);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) fn move_with_snapping(
    document: &Document,
    objects: &[Annotation],
    delta: [f32; 2],
    tolerance: Option<f32>,
    duplicate: bool,
) -> capture_core::Result<SnapResult> {
    move_with_candidates(document, objects, delta, tolerance, duplicate, None)
}
