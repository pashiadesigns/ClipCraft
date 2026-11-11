//! Sharing (approved beta 6 tab 3, `artifacts/beta6-proposal`): kits
//! (Share a kit, Add a kit), Preferences › Team (a team kit folder and
//! locks for shared projects), the read-only banner, and Pack project.
//! No accounts and no server: everything travels through files and folders
//! people already share.
use crate::tips::Tip as _;
use crate::{Editor, egui};
use capture_core::kit::{Category, Creative, Kit, Template, unique_name};
use capture_core::lock::{Lock, Opened};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const DIALOG_WIDTH: f32 = 440.;

pub(crate) struct ShareKit {
    name: String,
    include: [bool; 6],
    kit: Kit,
    error: String,
}
pub(crate) struct AddKit {
    file: String,
    kit: Kit,
    include: [bool; 6],
    /// Items in each category whose names match ones already here.
    same: [usize; 6],
    replace: bool,
    error: String,
}
enum Done {
    Packed(PathBuf),
    Unpacked(PathBuf),
}
struct Job {
    label: &'static str,
    cancel: Arc<AtomicBool>,
    /// Progress in thousandths.
    progress: Arc<AtomicU32>,
    result: Receiver<Result<Done, String>>,
}
#[derive(Default)]
pub(crate) struct Sharing {
    pub(crate) share: Option<ShareKit>,
    pub(crate) add: Option<AddKit>,
    /// Locks this PC holds on shared projects it has open.
    held: Vec<(PathBuf, Lock)>,
    /// Shared projects someone else is editing, opened read-only here.
    read_only: Vec<(PathBuf, Lock)>,
    job: Option<Job>,
    team_checked: Option<Instant>,
    locks_refreshed: Option<Instant>,
    /// Library cards' shared projects that someone else has open, by path,
    /// with who; read with the team folder check.
    card_locks: std::collections::HashMap<String, String>,
    /// A shared project just opened, to note on its Library item.
    pending_shared: Option<PathBuf>,
}
impl Drop for Sharing {
    /// Closing ClipDesigner closes every project, so its locks go too.
    fn drop(&mut self) {
        for (path, lock) in &self.held {
            capture_core::lock::release(path, lock);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn index(c: Category) -> usize {
    Category::ALL.iter().position(|x| *x == c).unwrap()
}
fn size_text(bytes: usize) -> String {
    if bytes < 1 << 20 {
        format!("about {} KB", bytes.div_ceil(1024).max(1))
    } else {
        format!("about {:.1} MB", bytes as f64 / (1 << 20) as f64)
    }
}
/// "updated today", "updated 3 days ago".
fn age_text(modified: std::time::SystemTime) -> String {
    let days = modified.elapsed().map_or(0, |d| d.as_secs() / 86_400);
    match days {
        0 => "updated today".into(),
        1 => "updated yesterday".into(),
        n => format!("updated {n} days ago"),
    }
}
fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// A folder other people can reach: a network share, a mapped network
/// drive, a cloud-synced folder (Google Drive, OneDrive, SharePoint,
/// Dropbox, Box) or anywhere under the team kit folder's parent.
pub(crate) fn shared_location(path: &Path, team_folder: Option<&Path>) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\") {
        return true;
    }
    if let Some(team) = team_folder
        && path.starts_with(team.parent().unwrap_or(team))
    {
        return true;
    }
    let synced = path.components().any(|c| {
        let part = c.as_os_str().to_string_lossy();
        let lower = part.to_ascii_lowercase();
        [
            "google drive",
            "my drive",
            "shared drives",
            "onedrive",
            "dropbox",
            "box",
            "sharepoint",
        ]
        .contains(&lower.as_str())
            || lower.starts_with("onedrive - ")
    });
    synced || network_drive(path)
}
#[cfg(windows)]
fn network_drive(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW};
    let Some(std::path::Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    let root: Vec<u16> =
        std::ffi::OsString::from(format!("{}\\", prefix.as_os_str().to_string_lossy()))
            .encode_wide()
            .chain([0])
            .collect();
    // DRIVE_REMOTE
    if unsafe { GetDriveTypeW(root.as_ptr()) } == 4 {
        return true;
    }
    // Google Drive for desktop mounts a drive labelled "Google Drive".
    let mut label = [0u16; 64];
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    let end = label.iter().position(|c| *c == 0).unwrap_or(label.len());
    ok != 0 && String::from_utf16_lossy(&label[..end]).eq_ignore_ascii_case("Google Drive")
}
#[cfg(not(windows))]
fn network_drive(_: &Path) -> bool {
    false
}

impl Editor {
    fn my_lock(&self) -> Lock {
        let team = self.quick_controls.team();
        let name = Some(team.display_name.trim().to_owned())
            .filter(|n| !n.is_empty())
            .or_else(|| std::env::var("USERNAME").ok())
            .unwrap_or_else(|| "Someone".into());
        Lock {
            name: name.chars().take(60).collect(),
            computer: std::env::var("COMPUTERNAME")
                .unwrap_or_else(|_| "this PC".into())
                .chars()
                .take(64)
                .collect(),
            since_ms: now_ms(),
        }
    }
    /// After a project opens from `path`: lock it if it's in a shared
    /// folder, or note that someone else has it.
    pub(crate) fn lock_opened_project(&mut self, path: &Path) {
        let team = self.quick_controls.team();
        let shared = team.lock_shared && shared_location(path, team.folder.as_deref());
        if !shared {
            return;
        }
        self.sharing.pending_shared = Some(path.to_owned());
        if self.sharing.held.iter().any(|(p, _)| p == path) {
            return;
        }
        self.sharing.read_only.retain(|(p, _)| p != path);
        let me = self.my_lock();
        match capture_core::lock::take(path, &me) {
            Opened::Mine => self.sharing.held.push((path.to_owned(), me)),
            Opened::Theirs(lock) => {
                self.status = format!(
                    "{} is editing this project. It's open read-only; Open a copy to make changes.",
                    lock.name
                );
                self.sharing.read_only.push((path.to_owned(), lock));
            }
        }
    }
    /// Someone else's lock on the open project, if it's open read-only.
    pub(crate) fn locked_by_other(&self) -> Option<&Lock> {
        let path = self.path.as_ref()?;
        self.sharing
            .read_only
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, l)| l)
    }
    /// Save refuses a read-only shared project and says why.
    pub(crate) fn refuse_locked_save(&mut self) -> bool {
        if let Some(lock) = self.locked_by_other() {
            self.status = format!(
                "{} is editing this project, so it can't be saved here. Use Open a copy or Save As.",
                lock.name
            );
            return true;
        }
        false
    }
    /// "Open a copy": the read-only project becomes a new Library item.
    pub(crate) fn open_lock_copy(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let label = path.file_stem().map_or_else(
            || "Project".into(),
            |s| format!("{} (copy)", s.to_string_lossy()),
        );
        self.sharing.read_only.retain(|(p, _)| *p != path);
        self.path = None;
        self.history.active = None;
        self.record_history(&label, capture_core::history::Origin::Project);
        self.status = format!("Opened a copy as \"{label}\" in your Library.");
    }
    /// The banner above a read-only shared project (approved mockup).
    pub(crate) fn show_lock_banner(&mut self, ui: &mut egui::Ui) {
        let Some(lock) = self.locked_by_other().cloned() else {
            return;
        };
        let mut copy = false;
        egui::Panel::top("shared-lock-banner")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(42, 38, 24))
                    .stroke(egui::Stroke::new(1., egui::Color32::from_rgb(92, 79, 34)))
                    .inner_margin(egui::Margin::symmetric(12, 7)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(14., 14.), egui::Sense::hover());
                    paint_lock(ui.painter(), r, egui::Color32::from_rgb(242, 201, 76));
                    let since = time_of_day(lock.since_ms);
                    ui.label(egui::RichText::new(format!("{} is editing this project", lock.name)).strong());
                    ui.label(format!(
                        "on {} since {since}. It's open read-only.",
                        lock.computer
                    ));
                    copy = ui
                        .link("Open a copy")
                        .tip("Open a copy · Make your own copy in the Library to change. The shared project stays as it is.")
                        .clicked();
                });
            });
        if copy {
            self.open_lock_copy();
        }
    }
    /// Keeps held locks fresh (they go stale after 12 hours) and checks the
    /// team kit folder now and then.
    pub(crate) fn sharing_tick(&mut self, ctx: &egui::Context) {
        if self
            .sharing
            .locks_refreshed
            .is_none_or(|t| t.elapsed() > Duration::from_secs(3600))
        {
            self.sharing.locks_refreshed = Some(Instant::now());
            let now = now_ms();
            for (path, lock) in &mut self.sharing.held {
                lock.since_ms = now;
                let _ = capture_core::lock::take(path, lock);
            }
        }
        if self
            .sharing
            .team_checked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
        {
            self.sharing.team_checked = Some(Instant::now());
            self.sync_team_kits(ctx);
            self.refresh_card_locks();
        }
        if self.sharing.pending_shared.is_some()
            && let Some(id) = self.history.active.clone()
            && let Some(path) = self.sharing.pending_shared.take()
            && let Some(store) = self.history.store.as_mut()
            && store.entries().iter().any(|e| {
                e.id == id && e.organize.shared.as_deref() != Some(&*path.to_string_lossy())
            })
        {
            let _ = store.set_shared(&id, Some(&path));
            self.refresh_card_locks();
        }
        self.poll_sharing_job(ctx);
    }
    /// Who else has a Library card's shared project open, if anyone.
    pub(crate) fn shared_holder(&self, shared: &str) -> Option<String> {
        self.sharing.card_locks.get(shared).cloned()
    }
    fn refresh_card_locks(&mut self) {
        let me = self.my_lock();
        let now = now_ms();
        self.sharing.card_locks = self
            .history
            .entries()
            .into_iter()
            .filter_map(|e| e.organize.shared)
            .filter_map(|shared| {
                let lock = capture_core::lock::read(Path::new(&shared), now)?;
                let theirs =
                    !(lock.computer.eq_ignore_ascii_case(&me.computer) && lock.name == me.name);
                theirs.then_some((shared, lock.name))
            })
            .collect();
    }
    /// Adds each kit in the team folder that is new or changed since it was
    /// last added, replacing items with the same names.
    pub(crate) fn sync_team_kits(&mut self, ctx: &egui::Context) {
        let mut team = self.quick_controls.team();
        let Some(folder) = team.folder.clone() else {
            return;
        };
        let mut added = vec![];
        for (name, modified) in team_kits(&folder) {
            let stamp = modified
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            if team.off.contains(&name) || team.added.get(&name) == Some(&stamp) {
                continue;
            }
            match Kit::read(&folder.join(&name)) {
                Ok(kit) => match self.add_kit(ctx, &kit, [true; 6], true) {
                    Ok(_) => {
                        team.added.insert(name, stamp);
                        added.push(kit.name);
                    }
                    Err(e) => self.status = format!("Team kit {name}: {e}"),
                },
                Err(e) => self.status = format!("Team kit {name}: {e}"),
            }
        }
        if !added.is_empty() {
            team.added.retain(|n, _| folder.join(n).is_file());
            if let Err(e) = self.quick_controls.set_team(team) {
                self.status = e;
            } else {
                self.status = format!("Updated from the team folder: {}.", added.join(", "));
            }
        }
    }

    // ------------------------------------------------------------ kits

    /// Everything this PC could share, before the person picks categories.
    pub(crate) fn gather_kit(&mut self, name: &str) -> Kit {
        let mut kit = Kit::new(name, &format!("ClipDesigner {}", env!("CARGO_PKG_VERSION")));
        if let Some(store) = self.presets.templates.as_ref() {
            kit.presets = store.entries().to_vec();
        }
        if let Some(store) = self.presets.arrow_store() {
            kit.arrow_styles = store.entries().to_vec();
        }
        if let Some(store) = self.presets.reusable.store_mut() {
            kit.creative = store
                .entries()
                .iter()
                .map(|e| Creative {
                    name: e.name.clone(),
                    payload: e.payload.clone(),
                })
                .collect();
        }
        if let Some(p) = crate::color_widgets::palette_store().and_then(|s| s.read().ok()) {
            kit.palettes = p.palettes;
        }
        kit.sensitive_types = self.quick_controls.sensitive_types();
        kit.watermarks = self.quick_controls.watermarks().0;
        if let Some(root) = self.template_root()
            && let Ok(library) = capture_core::templates::library::Library::open(&root)
        {
            for entry in library.0.entries().iter().take(32) {
                if let Ok(document) = library.0.load(&entry.id)
                    && document.video.is_none()
                {
                    kit.templates.push(Template {
                        name: entry.label.clone(),
                        document,
                    });
                }
            }
        }
        kit
    }
    pub(crate) fn open_share_kit(&mut self) {
        let kit = self.gather_kit("My kit");
        self.sharing.share = Some(ShareKit {
            name: "My kit".into(),
            include: Category::ALL.map(|c| c.shared_by_default() && kit.count(c) > 0),
            kit,
            error: String::new(),
        });
    }
    pub(crate) fn choose_kit_to_add(&mut self) {
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(self.dialog_directory())
            .add_filter("ClipDesigner kit", &[capture_core::kit::EXTENSION])
            .pick_file()
        {
            self.open_kit(&path);
        }
    }
    /// Add a kit: shows what it would add. Nothing changes until Add.
    pub(crate) fn open_kit(&mut self, path: &Path) {
        match Kit::read(path) {
            Ok(kit) => {
                let same = self.kit_conflicts(&kit);
                self.sharing.add = Some(AddKit {
                    file: path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    include: Category::ALL.map(|c| kit.count(c) > 0),
                    kit,
                    same,
                    replace: false,
                    error: String::new(),
                });
            }
            Err(e) => self.status = format!("This kit can't be added: {e}"),
        }
    }
    /// How many of a kit's items share a name with one already here.
    fn kit_conflicts(&mut self, kit: &Kit) -> [usize; 6] {
        let mine = self.gather_kit("mine");
        let mut clash = [0; 6];
        clash[index(Category::Presets)] = kit
            .presets
            .iter()
            .filter(|s| {
                mine.presets
                    .iter()
                    .any(|m| m.kind == s.kind && same(&m.name, &s.name))
            })
            .count()
            + kit
                .arrow_styles
                .iter()
                .filter(|s| mine.arrow_styles.iter().any(|m| same(&m.name, &s.name)))
                .count();
        for c in &kit.creative {
            let category = if matches!(c.payload, capture_core::reusable_presets::Payload::Group(_))
            {
                Category::Blocks
            } else {
                Category::Presets
            };
            if mine
                .creative
                .iter()
                .any(|m| same(&m.name, &c.name) && m.payload.category() == c.payload.category())
            {
                clash[index(category)] += 1;
            }
        }
        clash[index(Category::Palettes)] = kit
            .palettes
            .iter()
            .filter(|p| mine.palettes.iter().any(|m| same(&m.name, &p.name)))
            .count();
        clash[index(Category::SensitiveTypes)] = kit
            .sensitive_types
            .iter()
            .filter(|t| mine.sensitive_types.iter().any(|m| same(&m.name, &t.name)))
            .count();
        clash[index(Category::Watermarks)] = kit
            .watermarks
            .iter()
            .filter(|w| mine.watermarks.iter().any(|m| same(&m.name, &w.name)))
            .count();
        clash[index(Category::Templates)] = kit
            .templates
            .iter()
            .filter(|t| mine.templates.iter().any(|m| same(&m.name, &t.name)))
            .count();
        clash
    }
    /// Adds the chosen categories of `kit`. Items whose names are taken are
    /// renamed ("Brand (2)"), or replace the existing ones when `replace`.
    pub(crate) fn add_kit(
        &mut self,
        ctx: &egui::Context,
        kit: &Kit,
        include: [bool; 6],
        replace: bool,
    ) -> Result<usize, String> {
        let kit = kit.only(|c| include[index(c)]);
        let err = |e: capture_core::Error| e.to_string();
        let mut added = 0;
        if let Some(store) = self.presets.templates.as_mut() {
            for style in &kit.presets {
                let mut style = style.clone();
                if !replace {
                    let taken = |n: &str| {
                        store
                            .entries()
                            .iter()
                            .any(|s| s.kind == style.kind && same(&s.name, n))
                    };
                    style.name = unique_name(&style.name, taken);
                }
                store.save(style).map_err(err)?;
                added += 1;
            }
        }
        if let Some(store) = self.presets.arrow_store() {
            for style in &kit.arrow_styles {
                let mut style = style.clone();
                if !replace {
                    let taken = |n: &str| store.entries().iter().any(|s| same(&s.name, n));
                    style.name = unique_name(&style.name, taken);
                }
                store.save(style).map_err(err)?;
                added += 1;
            }
        }
        if !kit.creative.is_empty()
            && let Some(store) = self.presets.reusable.store_mut()
        {
            for c in &kit.creative {
                let existing = store
                    .entries()
                    .iter()
                    .find(|e| {
                        same(&e.name, &c.name) && e.payload.category() == c.payload.category()
                    })
                    .map(|e| e.id);
                match existing {
                    Some(id) if replace => {
                        store.remove(id).map_err(err)?;
                        store.save(&c.name, c.payload.clone()).map_err(err)?;
                    }
                    Some(_) => {
                        let category = c.payload.category();
                        let taken = |n: &str| {
                            store
                                .entries()
                                .iter()
                                .any(|e| same(&e.name, n) && e.payload.category() == category)
                        };
                        let name = unique_name(&c.name, taken);
                        store.save(&name, c.payload.clone()).map_err(err)?;
                    }
                    None => {
                        store.save(&c.name, c.payload.clone()).map_err(err)?;
                    }
                }
                added += 1;
            }
        }
        if !kit.palettes.is_empty()
            && let Some(store) = crate::color_widgets::palette_store()
        {
            use capture_core::palettes::Edit;
            let before = store.read().map_err(err)?;
            for p in &kit.palettes {
                if replace
                    && let Some(old) = before.palettes.iter().find(|o| same(&o.name, &p.name))
                {
                    store.edit(Edit::Select(old.name.clone())).map_err(err)?;
                    store.edit(Edit::Delete).map_err(err)?;
                }
                // Create picks "Brand 2" itself when the name is taken.
                store
                    .edit(Edit::Create {
                        name: p.name.clone(),
                        colors: p.colors.clone(),
                    })
                    .map_err(err)?;
                added += 1;
            }
            let now = store.read().map_err(err)?;
            let keep = if now.names().contains(&before.selected) {
                before.selected
            } else {
                capture_core::palettes::BUILT_IN.into()
            };
            store.edit(Edit::Select(keep)).map_err(err)?;
            crate::color_widgets::reload_palettes(ctx);
        }
        if !kit.sensitive_types.is_empty() {
            let mut list = self.quick_controls.sensitive_types();
            for t in &kit.sensitive_types {
                let mut t = t.clone();
                match list.iter().position(|m| same(&m.name, &t.name)) {
                    Some(i) if replace => list[i] = t,
                    _ => {
                        t.name = unique_name(&t.name, |n| list.iter().any(|m| same(&m.name, n)));
                        list.push(t);
                    }
                }
                added += 1;
            }
            self.quick_controls.set_sensitive_types(list)?;
        }
        if !kit.watermarks.is_empty() {
            let (mut list, default) = self.quick_controls.watermarks();
            for w in &kit.watermarks {
                let mut w = w.clone();
                match list.iter().position(|m| same(&m.name, &w.name)) {
                    Some(i) if replace => {
                        w.name = list[i].name.clone();
                        list[i] = w;
                    }
                    _ => {
                        w.name = unique_name(&w.name, |n| list.iter().any(|m| same(&m.name, n)));
                        list.push(w);
                    }
                }
                added += 1;
            }
            if list.len() > 32 {
                return Err(
                    "Keep up to 32 watermark presets. Remove some, then add the kit again.".into(),
                );
            }
            self.quick_controls.remember_watermarks(list, default);
        }
        if !kit.templates.is_empty() {
            let root = self
                .template_root()
                .ok_or("The template library isn't available.")?;
            let mut library =
                capture_core::templates::library::Library::open(&root).map_err(err)?;
            for t in &kit.templates {
                let existing = library
                    .0
                    .entries()
                    .iter()
                    .find(|e| same(&e.label, &t.name))
                    .cloned();
                match existing {
                    Some(entry) if replace => {
                        library.0.update(&entry, &t.document).map_err(err)?;
                    }
                    _ => {
                        let taken = |n: &str| library.0.entries().iter().any(|e| same(&e.label, n));
                        let name = unique_name(&t.name, taken);
                        library.save(&t.document, &name, false).map_err(err)?;
                    }
                }
                added += 1;
            }
            crate::template_picker::forget_catalog(ctx);
        }
        Ok(added)
    }
    pub(crate) fn show_sharing(&mut self, ctx: &egui::Context) {
        self.show_share_kit(ctx);
        self.show_add_kit(ctx);
        self.show_sharing_job(ctx);
    }
    fn show_share_kit(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.sharing.share.take() else {
            return;
        };
        let (mut save, mut cancel) = (false, false);
        let chosen = d.kit.only(|c| d.include[index(c)]);
        let count = chosen.total();
        let size = serde_json::to_vec(&chosen).map_or(0, |b| b.len());
        let modal = crate::controls::Modal::new(egui::Id::unique("share-kit")).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            crate::controls::dialog_title(ui, "Share a kit");
            ui.weak("A kit is one file with your styles and building blocks. Send it any way you like; whoever opens it chooses what to add.");
            crate::watermark_ui::row(ui, "Kit name", |ui| {
                ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut d.name)
                        .desired_width(ui.available_width()),
                )
                .tip("Kit name · What people see when they add the kit.");
            });
            crate::controls::dialog_section(ui, "Include", None);
            for c in Category::ALL {
                let n = d.kit.count(c);
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(n > 0, |ui| {
                        crate::controls::toggle(ui, &mut d.include[index(c)], c.label())
                            .tip(category_tip(c));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(n.to_string());
                    });
                });
            }
            if !d.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
            }
            ui.add_space(4.);
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), crate::controls::DIALOG_ACTION_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    save = ui
                        .add_enabled(count > 0, crate::controls::dialog_button("Save kit…", true))
                        .clicked();
                    cancel = ui.add(crate::controls::dialog_button("Cancel", false)).clicked();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.weak(format!(
                            "{count} item{} · {}",
                            if count == 1 { "" } else { "s" },
                            size_text(size)
                        ));
                    });
                },
            );
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if save {
            let mut kit = chosen;
            kit.name = d.name.trim().to_owned();
            self.suspend_global_capture();
            if let Some(mut path) = rfd::FileDialog::new()
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .set_file_name(format!(
                    "{}.{}",
                    capture_core::pack::file_stem(&kit.name),
                    capture_core::kit::EXTENSION
                ))
                .add_filter("ClipDesigner kit", &[capture_core::kit::EXTENSION])
                .save_file()
            {
                path.set_extension(capture_core::kit::EXTENSION);
                match kit.write(&path) {
                    Ok(()) => {
                        self.status = format!("Saved the kit {}", path.display());
                        return;
                    }
                    Err(e) => d.error = e.to_string(),
                }
            }
        }
        self.sharing.share = Some(d);
    }
    fn show_add_kit(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.sharing.add.take() else {
            return;
        };
        let (mut add, mut cancel) = (false, false);
        let count: usize = Category::ALL
            .iter()
            .filter(|c| d.include[index(**c)])
            .map(|c| d.kit.count(*c))
            .sum();
        let modal = crate::controls::Modal::new(egui::Id::unique("add-kit")).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            crate::controls::dialog_title(ui, "Add a kit");
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(40., 40.), egui::Sense::hover());
                ui.painter().rect_filled(r, 6., crate::shell::ACCENT);
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    "KIT",
                    egui::FontId::proportional(11.),
                    egui::Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&d.file).strong());
                    ui.weak(format!("Made with {}", d.kit.made_with));
                });
            });
            crate::controls::dialog_section(ui, "This kit adds", None);
            for c in Category::ALL {
                let n = d.kit.count(c);
                if n == 0 {
                    continue;
                }
                let same = d.same[index(c)];
                ui.horizontal(|ui| {
                    crate::controls::toggle(ui, &mut d.include[index(c)], &c.count_label(n))
                        .tip(category_tip(c));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(match same {
                            0 => "new".to_owned(),
                            1 => "1 replaces yours".into(),
                            n => format!("{n} replace yours"),
                        });
                    });
                });
            }
            if d.same.iter().any(|n| *n > 0) {
                crate::watermark_ui::row(ui, "Same names", |ui| {
                    ui.selectable_value(&mut d.replace, false, "Keep both")
                        .tip("Keep both · Items with names you already use are added with a number, such as Brand (2).");
                    ui.selectable_value(&mut d.replace, true, "Replace mine")
                        .tip("Replace mine · Items with names you already use replace yours.");
                });
            }
            if !d.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
            }
            ui.add_space(4.);
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), crate::controls::DIALOG_ACTION_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    add = ui
                        .add_enabled(
                            count > 0,
                            crate::controls::dialog_button(
                                &format!("Add {count} item{}", if count == 1 { "" } else { "s" }),
                                true,
                            ),
                        )
                        .clicked();
                    cancel = ui.add(crate::controls::dialog_button("Cancel", false)).clicked();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.weak("Nothing is changed until you add it.");
                    });
                },
            );
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if add {
            let kit = d.kit.clone();
            match self.add_kit(ctx, &kit, d.include, d.replace) {
                Ok(n) => {
                    self.status = format!(
                        "Added {n} item{} from {}.",
                        if n == 1 { "" } else { "s" },
                        kit.name
                    );
                    return;
                }
                Err(e) => d.error = e,
            }
        }
        self.sharing.add = Some(d);
    }

    // ------------------------------------------------------------ packs

    /// Save As › Pack project: the project and all its media in one file.
    pub(crate) fn pack_project(&mut self, out: PathBuf) {
        let document = self.session.document.clone();
        let title = out
            .file_stem()
            .map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
        self.start_job("Packing the project", move |stop, progress| {
            capture_core::pack::pack(&document, &title, &out, stop, &mut |p| progress(p))
                .map(|()| Done::Packed(out))
                .map_err(|e| e.to_string())
        });
    }
    /// Opening a pack unpacks it into a new folder beside it, then opens it.
    pub(crate) fn open_pack(&mut self, pack: PathBuf) {
        let into = capture_core::pack::default_folder(&pack);
        self.start_job("Unpacking the project", move |stop, _| {
            capture_core::pack::unpack(&pack, &into, stop)
                .map(Done::Unpacked)
                .map_err(|e| e.to_string())
        });
    }
    fn start_job(
        &mut self,
        label: &'static str,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(f32)) -> Result<Done, String> + Send + 'static,
    ) {
        if self.sharing.job.is_some() {
            self.status = "Wait for the current pack to finish.".into();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, result) = std::sync::mpsc::channel();
        let (stop, shown) = (cancel.clone(), progress.clone());
        std::thread::spawn(move || {
            let out = work(&|| stop.load(Ordering::Relaxed), &|p| {
                shown.store((p.clamp(0., 1.) * 1000.) as u32, Ordering::Relaxed)
            });
            let _ = tx.send(out);
        });
        self.sharing.job = Some(Job {
            label,
            cancel,
            progress,
            result,
        });
    }
    fn poll_sharing_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.sharing.job else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(100));
        let Ok(result) = job.result.try_recv() else {
            return;
        };
        self.sharing.job = None;
        match result {
            Ok(Done::Packed(path)) => {
                self.status = format!(
                    "Packed {}. Send this one file; opening it unpacks the project.",
                    path.display()
                );
            }
            Ok(Done::Unpacked(project)) => {
                self.status = format!(
                    "Unpacked into {}",
                    project.parent().unwrap_or(&project).display()
                );
                self.request_action(crate::FileAction::OpenPath(project), ctx);
            }
            Err(e) => self.status = e,
        }
    }
    fn show_sharing_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.sharing.job else {
            return;
        };
        let progress = job.progress.load(Ordering::Relaxed) as f32 / 1000.;
        let label = job.label;
        let mut cancel = false;
        crate::controls::Modal::new(egui::Id::unique("sharing-job")).show(ctx, |ui| {
            ui.set_width(360.);
            crate::controls::dialog_title(ui, label);
            ui.add(egui::ProgressBar::new(progress).show_percentage());
            ui.add_space(6.);
            cancel = ui
                .add(crate::controls::dialog_button("Cancel", false))
                .clicked();
        });
        if cancel {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// The Save As dialog's Pack project choice.
pub(crate) fn is_pack(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(capture_core::pack::EXTENSION))
}

fn category_tip(c: Category) -> &'static str {
    match c {
        Category::Presets => {
            "Presets · Saved object styles, arrow styles, motion and audio presets."
        }
        Category::Blocks => "Blocks · Saved groups of objects to insert.",
        Category::Palettes => {
            "Color palettes · Your named palettes. The ClipDesigner palette is always there."
        }
        Category::SensitiveTypes => {
            "Custom sensitive types · The types you added to Find Sensitive Info."
        }
        Category::Watermarks => "Watermarks · Saved watermark presets, with any logo they use.",
        Category::Templates => "Templates · Your saved guide templates, with their pictures.",
    }
}

/// Kits in a team folder, by file name, with when each last changed.
pub(crate) fn team_kits(folder: &Path) -> Vec<(String, std::time::SystemTime)> {
    let Ok(list) = std::fs::read_dir(folder) else {
        return vec![];
    };
    let mut kits: Vec<_> = list
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(capture_core::kit::EXTENSION))
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            (meta.is_file() && meta.len() < 96 << 20).then(|| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                )
            })
        })
        .take(64)
        .collect();
    kits.sort();
    kits
}

/// Preferences › Team (approved mockup): the team kit folder, then shared
/// projects.
pub(crate) fn team_section(ui: &mut egui::Ui, team: &mut capture_core::quick_preferences::Team) {
    crate::panel_ui::heading(ui, "Folder", "Team kit folder");
    ui.horizontal(|ui| {
        let mut shown = team
            .folder
            .as_ref()
            .map_or_else(String::new, |f| f.display().to_string());
        ui.add_enabled(
            false,
            crate::unicode_input::TextEdit::singleline(&mut shown)
                .hint_text("No team folder")
                .desired_width((ui.available_width() - 150.).max(120.)),
        );
        if ui
            .button("Browse…")
            .tip("Team kit folder · Choose a shared folder that holds your team's kits.")
            .clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            team.folder = Some(folder);
        }
        if team.folder.is_some()
            && ui
                .button("Clear")
                .tip("Clear · Stop loading kits from a team folder. What was added stays.")
                .clicked()
        {
            team.folder = None;
        }
    });
    ui.weak("Kits in this folder (Google Drive, a network share, a Teams or SharePoint folder) load automatically and update when the folder does. They are read-only here; copy one to change it.");
    if let Some(folder) = team.folder.clone() {
        let kits = team_kits(&folder);
        if kits.is_empty() {
            ui.weak("No kits in this folder yet.");
        }
        for (name, modified) in kits {
            let mut on = !team.off.contains(&name);
            ui.horizontal(|ui| {
                let label = name
                    .strip_suffix(&format!(".{}", capture_core::kit::EXTENSION))
                    .unwrap_or(&name)
                    .to_owned();
                if crate::controls::toggle(ui, &mut on, &label)
                    .tip("Team kit · Turn off to stop adding this kit's updates.")
                    .changed()
                {
                    if on {
                        team.off.retain(|n| *n != name);
                    } else {
                        team.off.push(name.clone());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(age_text(modified));
                });
            });
        }
    }
    crate::panel_ui::divider(ui);
    crate::panel_ui::heading(ui, "Lock", "Shared projects");
    crate::panel_ui::setting(
        ui,
        "Lock projects I open from shared folders",
        "Others see who has a project open and get it read-only, so nobody overwrites anyone. The lock clears when you close ClipDesigner, or after 12 hours.",
        &mut team.lock_shared,
    );
    crate::watermark_ui::row(ui, "Show my name as", |ui| {
        ui.add(
            crate::unicode_input::TextEdit::singleline(&mut team.display_name)
                .hint_text(std::env::var("USERNAME").unwrap_or_default())
                .char_limit(60)
                .desired_width(ui.available_width()),
        )
        .tip("Show my name as · The name others see on a project you have open.");
    });
}

pub(crate) fn paint_lock(painter: &egui::Painter, r: egui::Rect, color: egui::Color32) {
    let body = egui::Rect::from_min_max(
        egui::pos2(r.left() + 1., r.center().y - 1.),
        egui::pos2(r.right() - 1., r.bottom()),
    );
    painter.rect_filled(body, 2., color);
    painter.circle_stroke(
        egui::pos2(r.center().x, r.center().y - 1.),
        r.width() * 0.3,
        egui::Stroke::new(1.6, color),
    );
}

/// Minutes local time is ahead of UTC on this PC now.
#[cfg(windows)]
fn local_offset_minutes() -> i64 {
    use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let mut local = windows_sys::Win32::Foundation::SYSTEMTIME::default();
    let mut utc = windows_sys::Win32::Foundation::SYSTEMTIME::default();
    unsafe {
        GetLocalTime(&mut local);
        GetSystemTime(&mut utc);
    }
    let minutes = |t: &windows_sys::Win32::Foundation::SYSTEMTIME| {
        i64::from(t.wHour) * 60 + i64::from(t.wMinute)
    };
    // Offsets lie within ±14 hours, so a day boundary shows as a jump.
    let mut d = minutes(&local) - minutes(&utc);
    if d > 14 * 60 {
        d -= 1440;
    } else if d < -14 * 60 {
        d += 1440;
    }
    d
}
#[cfg(not(windows))]
fn local_offset_minutes() -> i64 {
    0
}
/// "2:14 PM" for a lock's start, in local time.
fn time_of_day(ms: u64) -> String {
    let minutes = (ms / 60_000) as i64 + local_offset_minutes();
    let of_day = minutes.rem_euclid(1440);
    let (h, m) = (of_day / 60, of_day % 60);
    let hour = match h % 12 {
        0 => 12,
        n => n,
    };
    format!("{hour}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
}

#[cfg(test)]
mod tests;
