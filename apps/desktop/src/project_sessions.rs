//! In-memory project sessions live until the process exits, including tray mode.
use crate::{Editor, Session, canvas_view::CanvasView};
use std::path::{Path, PathBuf};
#[derive(Default)]
pub(super) struct Sessions {
    parked: Vec<Parked>,
    source: Option<PathBuf>,
}
struct Parked {
    session: Session,
    path: Option<PathBuf>,
    source: Option<PathBuf>,
    history: Option<String>,
    view: CanvasView,
    video: crate::video_editor::State,
}
impl Sessions {
    pub(crate) fn retained_media(&self, deleting: &[String]) -> Vec<PathBuf> {
        self.parked
            .iter()
            .filter(|p| !p.history.as_ref().is_some_and(|id| deleting.contains(id)))
            .flat_map(|p| p.session.retained_media())
            .collect()
    }
    pub(crate) fn forget_deleted(&mut self, deleting: &[String]) {
        self.parked
            .retain(|p| !p.history.as_ref().is_some_and(|id| deleting.contains(id)));
    }
    pub(crate) fn forget_active(&mut self, history: Option<&str>) {
        self.parked
            .retain(|p| !history.is_some_and(|h| p.history.as_deref() == Some(h)));
        self.source = None;
    }
}
fn same_path(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_owned());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_owned());
    if cfg!(windows) {
        a.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
    } else {
        a == b
    }
}

impl Editor {
    pub(super) fn switch_session(
        &mut self,
        fallback: Session,
        history: Option<&str>,
        path: Option<&Path>,
    ) {
        if history.is_some_and(|id| self.history.active.as_deref() == Some(id))
            || path.is_some_and(|path| {
                self.path
                    .as_deref()
                    .into_iter()
                    .chain(self.project_sessions.source.as_deref())
                    .any(|p| same_path(p, path))
            })
        {
            return;
        }
        self.session.finish_gesture();
        let found = self.project_sessions.parked.iter().position(|p| {
            history.is_some_and(|id| p.history.as_deref() == Some(id))
                || path.is_some_and(|path| {
                    p.path
                        .as_deref()
                        .into_iter()
                        .chain(p.source.as_deref())
                        .any(|p| same_path(p, path))
                })
        });
        let incoming = found.map(|i| self.project_sessions.parked.remove(i));
        self.video.leave_project();
        let (session, next_path, source, next_history, view, video) = if let Some(p) = incoming {
            (p.session, p.path, p.source, p.history, p.view, p.video)
        } else {
            (
                fallback,
                path.filter(|p| {
                    capture_core::project_file::is_image_project(p)
                        || capture_core::project_file::is_video_project(p)
                })
                .map(Path::to_owned),
                path.map(Path::to_owned),
                history.map(str::to_owned),
                CanvasView::default(),
                crate::video_editor::State::default(),
            )
        };
        let old = Parked {
            session: std::mem::replace(&mut self.session, session),
            path: self.path.take(),
            source: self.project_sessions.source.take(),
            history: self.history.active.take(),
            view: std::mem::replace(&mut self.view, view),
            video: std::mem::replace(&mut self.video, video),
        };
        self.project_sessions.parked.push(old);
        self.path = next_path;
        self.group_feedback = None;
        self.project_sessions.source = source;
        self.history.active = next_history;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    fn session() -> Session {
        Session::new(capture_core::Document::blank(300, 200).unwrap()).unwrap()
    }
    #[test]
    fn project_switches_keep_independent_undo_redo_and_path_aliases() {
        let ctx = crate::egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let a = Path::new("C:/session-tests/guide 📝.iclip");
        let b = Path::new("C:/session-tests/other.png");
        e.switch_session(session(), None, Some(a));
        e.history.active = Some("history-a".into());
        let marker =
            capture_core::steps::marker(1, [80., 80.], 44, [0, 0, 0, 255], (1, false), [300, 200])
                .unwrap();
        e.session
            .execute(Command::Add {
                annotation: marker.clone(),
            })
            .unwrap();
        e.switch_session(session(), None, Some(b));
        e.history.active = Some("history-b".into());
        e.session
            .execute(Command::Add { annotation: marker })
            .unwrap();
        e.session.execute(Command::Undo).unwrap();
        e.switch_session(session(), Some("history-a"), None);
        assert_eq!(e.session.document.annotations.len(), 1);
        assert!(e.session.can_undo());
        e.session.execute(Command::Undo).unwrap();
        assert!(e.session.document.annotations.is_empty());
        e.switch_session(session(), None, Some(b));
        assert!(e.session.can_redo());
        assert!(e.path.is_none());
        e.session.execute(Command::Redo).unwrap();
        assert_eq!(e.session.document.annotations.len(), 1);
        e.switch_session(session(), None, Some(a));
        assert!(e.session.can_redo());
        assert_eq!(e.path.as_deref(), Some(a));
        e.switch_session(session(), None, Some(a));
        assert!(
            e.session.can_redo(),
            "reopening active file must not replace its session"
        );
        assert!(
            Sessions::default().parked.is_empty(),
            "new process starts without old Undo history"
        );
    }
}
