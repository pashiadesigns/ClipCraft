use super::*;
use capture_core::video::templates::{self, Template};
#[derive(Default)]
pub(crate) struct Catalog {
    checked: Option<Instant>,
    root: Option<PathBuf>,
    choices: Vec<(String, Document)>,
}
impl Draft {
    /// Even dimensions within the video limits, or why not.
    fn size(&self) -> Result<[u32; 2], String> {
        let read = |v: &str, name: &str| {
            v.trim()
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a positive whole number."))
        };
        let [w, h] = [read(&self.width, "Width")?, read(&self.height, "Height")?];
        if w < 64 || h < 64 || w.max(h) > 1920 || w.min(h) > 1080 {
            return Err(
                "Video sizes run from 64 px up to 1920 × 1080, landscape or portrait.".into(),
            );
        }
        Ok([w / 2 * 2, h / 2 * 2])
    }
    fn duration(&self) -> Result<u32, String> {
        match self.seconds.trim().parse::<u32>() {
            Ok(s) if (1..=3600).contains(&s) => Ok(s),
            _ => Err("Duration must be 1 to 3600 seconds.".into()),
        }
    }
}
/// How long pictures overlap so each fades into the next.
const PICTURE_FADE: u64 = 600;
fn picture_length(clips: &[capture_core::video::editor::Segment]) -> u64 {
    let total: u64 = clips.iter().map(|c| c.duration()).sum();
    total.saturating_sub(PICTURE_FADE * (clips.len() as u64).saturating_sub(1))
}
/// Puts the pictures' track above the background and overlaps neighbours, so
/// they crossfade (clips that overlap on one track always do).
fn arrange_pictures(p: &mut capture_core::video::editor::Project, track: u64) {
    use capture_core::video::links::Layer;
    let mut at = 0;
    let mut order: Vec<usize> = (0..p.segments.len())
        .filter(|i| p.segments[*i].track == track)
        .collect();
    order.sort_by_key(|i| p.segments[*i].at);
    for i in order {
        p.segments[i].at = at;
        at += p.segments[i].duration().saturating_sub(PICTURE_FADE);
    }
    p.media_tracks.retain(|t| *t != track);
    p.media_tracks.insert(0, track);
    let mut layers = p.layers();
    layers.retain(|l| *l != Layer::Media(track));
    let first = layers
        .iter()
        .position(|l| matches!(l, Layer::Media(_)))
        .unwrap_or(layers.len());
    layers.insert(first, Layer::Media(track));
    p.layer_order = layers;
}
#[test]
fn pictures_sit_above_the_background_and_fade_into_each_other() {
    use capture_core::video::editor::{MediaKind, Segment, Source};
    let doc = templates::build("bg.png".into(), [640, 360], 13_800, 30, Template::Blank).unwrap();
    let mut p = doc.video.unwrap();
    let background = p.media_tracks[0];
    let track = p.add_media_track();
    let clips: Vec<_> = (0..3)
        .map(|i| {
            let source = Source {
                path: format!("picture-{i}.png"),
                size: [640, 360],
                duration: 3_600_000,
                kind: MediaKind::Image,
            };
            Segment::new(1, 0, 1, Some(source), 5000)
        })
        .collect();
    assert_eq!(picture_length(&clips), 13_800);
    p = imports::plan_import(&p, &clips, 0, track);
    arrange_pictures(&mut p, track);
    let mut pictures: Vec<_> = p.segments.iter().filter(|c| c.track == track).collect();
    pictures.sort_by_key(|c| c.at);
    assert_eq!(
        pictures.iter().map(|c| c.at).collect::<Vec<_>>(),
        [0, 4400, 8800]
    );
    assert!(
        pictures[0].until() > pictures[1].at,
        "they overlap to crossfade"
    );
    assert_eq!(
        p.media_tracks[0], track,
        "pictures draw above the background"
    );
    let layers = p.layers();
    let at = |l| layers.iter().position(|x| *x == l).unwrap();
    use capture_core::video::links::Layer;
    assert!(at(Layer::Media(track)) < at(Layer::Media(background)));
}
/// The first picture's shape, within the video size limits and with even sides.
fn images_video_size([width, height]: [u32; 2]) -> [u32; 2] {
    let scale = (1920. / width.max(height) as f64)
        .min(1080. / width.min(height) as f64)
        .min(1.);
    [width, height].map(|side| {
        let side = (side as f64 * scale).round() as u32;
        side.max(64) / 2 * 2
    })
}
#[test]
fn a_video_from_images_takes_the_first_pictures_shape() {
    assert_eq!(images_video_size([3840, 2160]), [1920, 1080]);
    assert_eq!(images_video_size([1000, 501]), [1000, 500]);
    assert_eq!(images_video_size([1080, 2400]), [864, 1920]);
    assert_eq!(images_video_size([40, 40]), [64, 64]);
}
pub(crate) struct Draft {
    template: Template,
    width: String,
    height: String,
    seconds: String,
    color: [u8; 4],
    fps: u32,
    error: String,
    preview: Option<(Template, [u32; 2], egui::TextureHandle)>,
}
impl Editor {
    pub(crate) fn new_video(&mut self, template: Template) {
        self.video.creation = Some(Draft {
            template,
            width: "1920".into(),
            height: "1080".into(),
            seconds: "15".into(),
            color: self.quick_controls.blank_defaults().video_color,
            fps: 30,
            error: String::new(),
            preview: None,
        });
    }
    /// Starts a new, unsaved video project from an existing video file.
    /// The file itself is only read; saving asks for a project location.
    pub(crate) fn new_video_from_file(&mut self) {
        self.video.pause();
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a video for a new project")
            .set_directory(self.dialog_directory())
            .add_filter("Videos", &["mp4", "mov", "m4v", "wmv", "avi"])
            .pick_file()
        {
            self.shell.backstage = None;
            self.open_video(path);
        }
    }
    /// Starts a new, unsaved video that shows the chosen pictures in order.
    pub(crate) fn new_video_from_images(&mut self, ctx: &egui::Context) {
        self.video.pause();
        self.suspend_global_capture();
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose images for a new video")
            .set_directory(self.dialog_directory())
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_files()
        else {
            return;
        };
        let color = self.quick_controls.blank_defaults().video_color;
        match self.images_document(&paths, color) {
            Ok(session) => {
                self.shell.backstage = None;
                self.request_action(
                    FileAction::Template {
                        session,
                        title: "Video from images".into(),
                    },
                    ctx,
                );
            }
            Err(e) => self.status = e,
        }
    }
    fn images_document(&self, paths: &[PathBuf], color: [u8; 4]) -> Result<Session, String> {
        // The media reader needs its own apartment, so it can't run on this thread.
        let list = paths.to_vec();
        let clips = std::thread::spawn(move || imports::import_files(&list))
            .join()
            .map_err(|_| "Could not read those images.".to_owned())??;
        let first = clips
            .first()
            .and_then(|c| c.source.as_ref())
            .ok_or("Choose at least one image.")?
            .size;
        let length = picture_length(&clips);
        let size = images_video_size(first);
        let folder = self
            .history
            .root()
            .ok_or("Set a project folder in Settings first.")?
            .join("backgrounds");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = folder.join(format!("Background-{stamp}.png"));
        let mut doc = templates::build(
            path.to_string_lossy().into(),
            size,
            length.max(1000),
            30,
            Template::Blank,
        )
        .map_err(|e| e.to_string())?;
        image::RgbaImage::from_pixel(doc.width, doc.height, image::Rgba(color))
            .save(&path)
            .map_err(|e| e.to_string())?;
        capture_core::history::media_cleanup::mark_created(&path).map_err(|e| e.to_string())?;
        let project = doc.video.as_mut().ok_or("Could not start the video.")?;
        let track = project.add_media_track();
        *project = imports::plan_import(project, &clips, 0, track);
        arrange_pictures(project, track);
        Session::unsaved(doc).map_err(|e| e.to_string())
    }
    /// Saved local video templates, re-read at most every five seconds.
    pub(crate) fn saved_video_templates(&mut self) -> Vec<(String, Document)> {
        let root = self.template_root();
        if self.video.template_catalog.root != root
            || self
                .video
                .template_catalog
                .checked
                .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
        {
            let choices = root
                .as_ref()
                .and_then(|root| capture_core::templates::library::Library::open(root).ok())
                .map(|store| {
                    store
                        .0
                        .entries()
                        .iter()
                        .filter_map(|e| {
                            store
                                .0
                                .load(&e.id)
                                .ok()
                                .filter(|d| d.video.is_some())
                                .map(|d| (e.label.clone(), d))
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.video.template_catalog = Catalog {
                root,
                checked: Some(Instant::now()),
                choices,
            };
        }
        self.video.template_catalog.choices.clone()
    }
    pub(crate) fn show_new_video(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.video.creation.take() else {
            return;
        };
        let mut create = false;
        let mut cancel = false;
        let modal =
            crate::controls::Modal::new(egui::Id::unique("new-video-project")).show(ctx, |ui| {
                // Approved bk-proposal: the same layout as New Image.
                ui.set_width(346.);
                ui.set_max_width(346.);
                crate::controls::dialog_title(ui, "New Video");
                let blank = d.template == Template::Blank;
                if !blank {
                    ui.weak(format!("Starting from {}", d.template.label()));
                }
                crate::controls::dialog_section(ui, "Size", None);
                let field = |ui: &mut egui::Ui, value: &mut String| {
                    ui.add(
                        crate::unicode_input::TextEdit::singleline(value)
                            .desired_width(72.)
                            .min_size(egui::vec2(72., 20.))
                            .align(egui::Align2::RIGHT_CENTER),
                    );
                };
                egui::Grid::new("new-video-size")
                    .min_col_width(64.)
                    .spacing([8., 8.])
                    .show(ui, |ui| {
                        ui.label("Width");
                        field(ui, &mut d.width);
                        ui.label("px");
                        ui.end_row();
                        ui.label("Height");
                        field(ui, &mut d.height);
                        ui.label("px");
                        ui.end_row();
                    });
                let current = [
                    d.width.trim().parse().unwrap_or(0),
                    d.height.trim().parse().unwrap_or(0),
                ];
                if let Some([w, h]) =
                    crate::blank_dialog::presets(ui, &crate::blank_dialog::VIDEO_PRESETS, current)
                {
                    d.width = w.to_string();
                    d.height = h.to_string();
                }
                if blank {
                    crate::controls::dialog_section(ui, "Background", None);
                    ui.horizontal(|ui| {
                        ui.add_sized([64., 20.], egui::Label::new("Canvas"));
                        crate::color_widgets::canvas_rgba(ui, &mut d.color);
                    });
                }
                crate::controls::dialog_section(ui, "Timing", None);
                egui::Grid::new("new-video-timing")
                    .min_col_width(64.)
                    .spacing([8., 8.])
                    .show(ui, |ui| {
                        ui.label("Duration");
                        field(ui, &mut d.seconds);
                        ui.label("seconds");
                        ui.end_row();
                        ui.label("Frame rate");
                        ui.horizontal(|ui| {
                            for fps in [15, 30, 60] {
                                ui.selectable_value(&mut d.fps, fps, fps.to_string());
                            }
                        });
                        ui.label("fps");
                        ui.end_row();
                    });
                let size = d.size();
                let valid = size.is_ok() && d.duration().is_ok();
                if let Some(e) = size.as_ref().err().cloned().or_else(|| d.duration().err()) {
                    ui.colored_label(egui::Color32::from_rgb(255, 170, 160), e);
                }
                if blank {
                    crate::blank_dialog::preview(ui, size.as_ref().ok().copied(), Some(d.color));
                } else if let Ok(size) = size {
                    if d.preview
                        .as_ref()
                        .is_none_or(|(t, s, _)| *t != d.template || *s != size)
                        && let Ok(mut doc) =
                            templates::build("preview.png".into(), size, 15000, d.fps, d.template)
                    {
                        doc.annotations = capture_core::video::editor::objects_at(&doc, 800);
                        doc.video = None;
                        let scale = (266. / size[0] as f64).min(74. / size[1] as f64);
                        let thumb = [
                            (size[0] as f64 * scale).ceil() as u32,
                            (size[1] as f64 * scale).ceil() as u32,
                        ];
                        if let Ok(list) = doc.display_list()
                            && let Ok(pixels) =
                                list.render_viewport(capture_core::display::Viewport {
                                    size: thumb,
                                    origin: [0., 0.],
                                    scale,
                                })
                        {
                            d.preview = Some((
                                d.template,
                                size,
                                ctx.load_texture(
                                    "video-starter-preview",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [thumb[0] as usize, thumb[1] as usize],
                                        pixels.as_raw(),
                                    ),
                                    egui::TextureOptions::LINEAR,
                                ),
                            ));
                        }
                    }
                    if let Some((_, _, texture)) = &d.preview {
                        ui.vertical_centered(|ui| ui.image((texture.id(), texture.size_vec2())));
                    }
                }
                ui.weak("The project grows with your content; everything stays editable.");
                if !d.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
                }
                (create, cancel) =
                    crate::controls::dialog_actions(ui, "Create", "Cancel", valid, false);
            });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if create {
            let blank_color = d.color;
            let mut defaults = self.quick_controls.blank_defaults();
            defaults.video_color = blank_color;
            self.quick_controls.remember_blank(defaults);
            let size = d.size();
            let seconds = d.duration();
            let run = || -> Result<Document, String> {
                let size = size?;
                let seconds = seconds?;
                let folder = self
                    .history
                    .root()
                    .ok_or("Set a project folder in Settings first.")?
                    .join("backgrounds");
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let path = folder.join(format!("Background-{stamp}.png"));
                let doc = templates::build(
                    path.to_string_lossy().into(),
                    size,
                    seconds as u64 * 1000,
                    d.fps,
                    d.template,
                )
                .map_err(|e| e.to_string())?;
                let color = if d.template == Template::Blank {
                    blank_color
                } else {
                    [27, 36, 42, 255]
                };
                image::RgbaImage::from_pixel(doc.width, doc.height, image::Rgba(color))
                    .save(&path)
                    .map_err(|e| e.to_string())?;
                capture_core::history::media_cleanup::mark_created(&path)
                    .map_err(|e| e.to_string())?;
                Ok(doc)
            };
            match run().and_then(|doc| Session::unsaved(doc).map_err(|e| e.to_string())) {
                Ok(session) => {
                    self.request_action(
                        FileAction::Template {
                            session,
                            title: d.template.label().into(),
                        },
                        ctx,
                    );
                    return;
                }
                Err(e) => d.error = e,
            }
        }
        self.video.creation = Some(d);
    }
}
