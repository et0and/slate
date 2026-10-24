//! Application state and workflows. Panels live in `library`, `sidebar`,
//! `inspector` and `overlay`; this module owns the data and the async bridges to
//! native dialogs and the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, CursorStyle, ExternalPaths, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, PathPromptOptions, Render, RenderImage, Window, WindowAppearance,
    actions, div, prelude::*, px,
};
use image::{Frame, RgbaImage};
use slate_core::PreviewImage;
use slate_core::batch::{self, RenameOptions, RenamePlan};
use slate_core::import::{self, Probe};
use slate_core::layout::{self, SheetLayout};
use slate_core::model::{Filter, Photo, PhotoId, Project, SheetId, SheetSettings};
use slate_core::pdf::{self, ExportSummary};

use crate::text_field::TextField;
use crate::theme;

actions!(
    contact_sheet_actions,
    [
        Quit,
        OpenProject,
        SaveProject,
        ExportSheet,
        ImportFiles,
        ImportFolder,
        SelectAllPhotos,
        DeselectPhotos,
        Dismiss,
    ]
);

#[derive(Debug, Clone)]
pub enum ThumbState {
    Ready(Arc<RenderImage>),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizeTarget {
    Left,
    Right,
}

const LEFT_MIN: f32 = 230.0;
const LEFT_MAX: f32 = 430.0;
const RIGHT_MIN: f32 = 320.0;
const RIGHT_MAX: f32 = 520.0;
const CENTER_MIN: f32 = 420.0;

#[derive(Debug, Default)]
pub struct ImportState {
    pub total: usize,
    pub done: usize,
    pub errors: Vec<String>,
}

pub struct ContactSheet {
    pub(crate) project: Project,
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) generation: u64,

    pub(crate) selected: HashSet<PhotoId>,
    pub(crate) filter: Filter,
    pub(crate) search: String,

    pub(crate) thumbs: HashMap<PhotoId, ThumbState>,
    pub(crate) thumb_queue: VecDeque<(PhotoId, PathBuf)>,
    pub(crate) thumbs_in_flight: usize,
    pub(crate) thumbs_queued: HashSet<PhotoId>,
    pub(crate) rotated: HashMap<PhotoId, Arc<RenderImage>>,
    pub(crate) rotate_queue: VecDeque<(PhotoId, PathBuf)>,
    pub(crate) rotate_in_flight: usize,
    pub(crate) rotate_queued: HashSet<PhotoId>,

    pub(crate) import_queue: VecDeque<PathBuf>,
    pub(crate) importing_paths: HashSet<PathBuf>,
    pub(crate) imports_in_flight: usize,
    pub(crate) import_state: Option<ImportState>,

    pub(crate) preview_page: usize,
    pub(crate) rename_pending: Option<RenamePlan>,
    pub(crate) rename_options: RenameOptions,
    pub(crate) exporting: Option<String>,

    pub(crate) status: String,
    pub(crate) log: VecDeque<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) focused_once: bool,
    pub(crate) window_title: String,
    pub(crate) dark: bool,
    pub(crate) appearance_initialized: bool,
    pub(crate) left_width: f32,
    pub(crate) right_width: f32,
    pub(crate) resizing: Option<ResizeTarget>,

    pub(crate) search_field: gpui::Entity<TextField>,
    pub(crate) title_field: gpui::Entity<TextField>,
    pub(crate) prefix_field: gpui::Entity<TextField>,
    pub(crate) suffix_field: gpui::Entity<TextField>,
    pub(crate) tag_field: gpui::Entity<TextField>,
}

impl ContactSheet {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();

        let search_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Filter file names").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.search = value.to_string();
                    this.preview_page = 0;
                    this.refresh_preview(cx);
                    cx.notify();
                });
            })
        });

        let title_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Sheet title").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.project.sheet.title = value.to_string();
                    this.dirty = true;
                    cx.notify();
                });
            })
        });

        let prefix_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Prefix").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.rename_options.prefix = value.to_string();
                    cx.notify();
                });
            })
        });

        let suffix_field = cx.new(|cx| {
            let change_weak = weak.clone();
            let submit_weak = weak.clone();
            TextField::new(cx, "Suffix")
                .on_change(move |value, app| {
                    let _ = change_weak.update(app, |this, cx| {
                        this.rename_options.suffix = value.to_string();
                        cx.notify();
                    });
                })
                .on_submit(move |_, app| {
                    let _ = submit_weak.update(app, |this, cx| this.preview_rename(cx));
                })
        });

        let tag_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Tag, e.g. street").on_submit(move |value, app| {
                let value = value.to_string();
                let _ = weak.update(app, |this, cx| this.apply_tag_value(value, cx, true));
            })
        });

        let mut log = VecDeque::new();
        log.push_back("Session ready".to_string());

        Self {
            project: Project::new("Untitled"),
            project_path: None,
            dirty: false,
            generation: 1,
            selected: HashSet::new(),
            filter: Filter::All,
            search: String::new(),
            thumbs: HashMap::new(),
            thumb_queue: VecDeque::new(),
            thumbs_in_flight: 0,
            thumbs_queued: HashSet::new(),
            rotated: HashMap::new(),
            rotate_queue: VecDeque::new(),
            rotate_in_flight: 0,
            rotate_queued: HashSet::new(),
            import_queue: VecDeque::new(),
            importing_paths: HashSet::new(),
            imports_in_flight: 0,
            import_state: None,
            preview_page: 0,
            rename_pending: None,
            rename_options: RenameOptions::default(),
            exporting: None,
            status: "Import files or a folder to begin".to_string(),
            log,
            focus: cx.focus_handle(),
            focused_once: false,
            window_title: String::new(),
            dark: false,
            appearance_initialized: false,
            left_width: 256.0,
            right_width: 344.0,
            resizing: None,
            search_field,
            title_field,
            prefix_field,
            suffix_field,
            tag_field,
        }
    }

    // ----- queries -------------------------------------------------------

    pub(crate) fn visible_indices(&self) -> Vec<usize> {
        self.project.filter_indices(&self.filter, &self.search)
    }

    pub(crate) fn visible_count(&self) -> usize {
        self.visible_indices().len()
    }

    pub(crate) fn working_count(&self) -> usize {
        if self.selected.is_empty() {
            self.visible_count()
        } else {
            self.project
                .photos
                .iter()
                .filter(|photo| self.selected.contains(&photo.id))
                .count()
        }
    }

    pub(crate) fn page_count(&self) -> usize {
        self.working_count()
            .div_ceil(self.project.sheet.per_page())
            .max(1)
    }

    pub(crate) fn display_page_count(&self) -> usize {
        if self.working_count() == 0 {
            0
        } else {
            self.page_count()
        }
    }

    pub(crate) fn current_page(&self) -> usize {
        self.preview_page.min(self.page_count().saturating_sub(1))
    }

    /// Explicit selection wins. With no selection, every visible photo is the
    /// working set, so import -> export needs no extra step.
    pub(crate) fn export_photos(&self) -> Vec<Photo> {
        let selected = if self.selected.is_empty() {
            self.visible_indices()
                .into_iter()
                .map(|index| self.project.photos[index].id)
                .collect::<HashSet<_>>()
        } else {
            self.selected.clone()
        };
        self.project
            .photos
            .iter()
            .filter(|photo| selected.contains(&photo.id))
            .cloned()
            .collect()
    }

    pub(crate) fn target_ids(&self) -> Vec<PhotoId> {
        if self.selected.is_empty() {
            self.visible_indices()
                .into_iter()
                .map(|index| self.project.photos[index].id)
                .collect()
        } else {
            self.project
                .photos
                .iter()
                .filter(|photo| self.selected.contains(&photo.id))
                .map(|photo| photo.id)
                .collect()
        }
    }

    pub(crate) fn filter_label(&self) -> String {
        match &self.filter {
            Filter::All => "All images".to_string(),
            Filter::Untagged => "Untagged".to_string(),
            Filter::Tag(tag) => format!("#{tag}"),
        }
    }

    pub(crate) fn summary(&self) -> String {
        let photos = self.working_count();
        let layout = SheetLayout::from_settings(&self.project.sheet);
        let pages = self.page_count();
        format!(
            "{} photos / {} page{} / {} per page",
            photos,
            pages,
            if pages == 1 { "" } else { "s" },
            layout.per_page()
        )
    }

    // ----- status --------------------------------------------------------

    pub(crate) fn log(&mut self, line: impl Into<String>) {
        self.log.push_back(line.into());
        while self.log.len() > 7 {
            self.log.pop_front();
        }
    }

    pub(crate) fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    fn clamp_panel_widths(&mut self, window: &Window) {
        let total = f32::from(window.viewport_size().width);
        let max_left = (total - self.right_width - CENTER_MIN).clamp(LEFT_MIN, LEFT_MAX);
        self.left_width = self.left_width.clamp(LEFT_MIN, max_left);
        let max_right = (total - self.left_width - CENTER_MIN).clamp(RIGHT_MIN, RIGHT_MAX);
        self.right_width = self.right_width.clamp(RIGHT_MIN, max_right);
    }

    fn begin_resize(&mut self, target: ResizeTarget, cx: &mut Context<Self>) {
        self.resizing = Some(target);
        cx.notify();
    }

    fn resize_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.resizing else {
            return;
        };
        let total = f32::from(window.viewport_size().width);
        let x = f32::from(event.position.x);
        match target {
            ResizeTarget::Left => {
                let max = (total - self.right_width - CENTER_MIN).clamp(LEFT_MIN, LEFT_MAX);
                self.left_width = x.clamp(LEFT_MIN, max);
            }
            ResizeTarget::Right => {
                let max = (total - self.left_width - CENTER_MIN).clamp(RIGHT_MIN, RIGHT_MAX);
                self.right_width = (total - x).clamp(RIGHT_MIN, max);
            }
        }
        cx.notify();
    }

    fn end_resize(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.resizing.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        theme::set_dark(self.dark);
        let fields = [
            self.search_field.clone(),
            self.title_field.clone(),
            self.prefix_field.clone(),
            self.suffix_field.clone(),
            self.tag_field.clone(),
        ];
        for field in fields {
            field.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    pub(crate) fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_page = self.preview_page.min(self.page_count().saturating_sub(1));
        self.schedule_preview_rotations(cx);
    }

    // ----- selection -----------------------------------------------------

    pub(crate) fn toggle_selected(&mut self, id: PhotoId, cx: &mut Context<Self>) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn select_all_visible(&mut self, cx: &mut Context<Self>) {
        for index in self.visible_indices() {
            self.selected.insert(self.project.photos[index].id);
        }
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filter = Filter::All;
        self.search.clear();
        self.search_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn deselect_all(&mut self, cx: &mut Context<Self>) {
        self.selected.clear();
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn invert_visible(&mut self, cx: &mut Context<Self>) {
        for index in self.visible_indices() {
            let id = self.project.photos[index].id;
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
        }
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn remove_selected(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.set_status("Select photos to remove from the project");
            cx.notify();
            return;
        }
        let ids = self.target_ids();
        if ids.is_empty() {
            self.set_status("Select photos first");
            cx.notify();
            return;
        }
        let removed = self.project.remove_photos(&ids);
        for id in &ids {
            self.selected.remove(id);
            self.thumbs.remove(id);
            self.thumbs_queued.remove(id);
            self.rotated.remove(id);
        }
        self.dirty = true;
        self.log(format!("Removed {removed} photos from the project"));
        self.set_status(format!(
            "Removed {removed} from the project / files untouched"
        ));
        self.refresh_preview(cx);
        cx.notify();
    }

    // ----- tags ----------------------------------------------------------

    pub(crate) fn apply_tag(&mut self, cx: &mut Context<Self>, clear_field: bool) {
        let tag = self.tag_field.read(cx).value().to_string();
        self.apply_tag_value(tag, cx, clear_field);
    }

    pub(crate) fn apply_tag_value(
        &mut self,
        tag: String,
        cx: &mut Context<Self>,
        clear_field: bool,
    ) {
        let ids = self.target_ids();
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            self.set_status("Type a tag first");
            cx.notify();
            return;
        }
        if ids.is_empty() {
            self.set_status("No photos in scope");
            cx.notify();
            return;
        }
        let changed = batch::add_tag(&mut self.project, &ids, &tag);
        if changed > 0 {
            self.dirty = true;
        }
        if clear_field {
            self.tag_field
                .update(cx, |field, cx| field.set_value("", cx));
        }
        self.set_status(format!("Tagged {changed} / {tag} / {} in scope", ids.len()));
        self.log(format!("Tag +{tag} / {changed} photo(s)"));
        cx.notify();
    }

    pub(crate) fn remove_tag(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        let tag = self.tag_field.read(cx).value().trim().to_string();
        if tag.is_empty() {
            self.set_status("Type the tag to remove");
            cx.notify();
            return;
        }
        let changed = batch::remove_tag(&mut self.project, &ids, &tag);
        if changed > 0 {
            self.dirty = true;
        }
        self.tag_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.set_status(format!("Removed tag {tag} from {changed} photo(s)"));
        self.log(format!("Tag -{tag} / {changed} photo(s)"));
        cx.notify();
    }

    pub(crate) fn clear_tags(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        let changed = batch::clear_tags(&mut self.project, &ids);
        if changed > 0 {
            self.dirty = true;
        }
        self.set_status(format!("Cleared tags on {changed} photo(s)"));
        self.log(format!("Tags cleared / {changed} photo(s)"));
        self.refresh_preview(cx);
        cx.notify();
    }

    // ----- rename --------------------------------------------------------

    pub(crate) fn preview_rename(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        if ids.is_empty() {
            self.set_status("Select photos to rename");
            cx.notify();
            return;
        }
        let plan = batch::plan_rename(&self.project, &ids, &self.rename_options);
        if plan.entries.is_empty() {
            self.set_status("No changes / names already match");
            self.log(format!("Rename: 0 changes / {} skipped", plan.skipped));
            cx.notify();
            return;
        }
        let conflicts = plan.conflicts().count();
        if conflicts > 0 {
            self.set_status(format!("{conflicts} rename conflict(s)"));
        } else {
            self.set_status(format!("Rename ready / {} files", plan.entries.len()));
        }
        self.rename_pending = Some(plan);
        cx.notify();
    }

    pub(crate) fn apply_rename(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.rename_pending.take() else {
            return;
        };
        if !plan.can_apply() {
            self.rename_pending = Some(plan);
            self.set_status("Fix rename conflicts first");
            cx.notify();
            return;
        }
        let report = batch::apply_rename(&mut self.project, &plan);
        if report.renamed_count() > 0 {
            self.dirty = true;
        }
        for (_, error) in &report.errors {
            self.log(format!("Rename error: {error}"));
        }
        if report.renamed_count() > 0 {
            if let Some(path) = self.project_path.clone() {
                self.write_project(path, cx);
            } else {
                self.log("Rename applied / project unsaved");
            }
        }
        self.set_status(format!(
            "Renamed {} file(s){}",
            report.renamed_count(),
            if report.errors.is_empty() {
                String::new()
            } else {
                format!(" / {} error(s)", report.errors.len())
            }
        ));
        self.log(format!(
            "Rename / {} ok / {} error",
            report.renamed_count(),
            report.errors.len()
        ));
        cx.notify();
    }

    pub(crate) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename_pending = None;
        self.set_status("Rename cancelled");
        cx.notify();
    }

    // ----- sheet settings ------------------------------------------------

    pub(crate) fn mark_sheet_changed(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.normalize();
        self.dirty = true;
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn cycle_paper(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.paper = self.project.sheet.paper.next();
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_orientation(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.orientation = self.project.sheet.orientation.toggled();
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_columns(&mut self, delta: i8, cx: &mut Context<Self>) {
        let value = (i16::from(self.project.sheet.columns) + i16::from(delta)).clamp(1, 12);
        self.project.sheet.columns = value as u8;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_rows(&mut self, delta: i8, cx: &mut Context<Self>) {
        let value = (i16::from(self.project.sheet.rows) + i16::from(delta)).clamp(1, 12);
        self.project.sheet.rows = value as u8;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_margin(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.project.sheet.margin_mm = (self.project.sheet.margin_mm + delta).clamp(2.0, 40.0);
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_gutter(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.project.sheet.gutter_mm = (self.project.sheet.gutter_mm + delta).clamp(0.0, 20.0);
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_captions(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.captions = !self.project.sheet.captions;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_index(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.index = !self.project.sheet.index;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_rotate(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.rotate = !self.project.sheet.rotate;
        self.rotated.clear();
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_header(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.header = !self.project.sheet.header;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_footer(&mut self, cx: &mut Context<Self>) {
        self.project.sheet.footer = !self.project.sheet.footer;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn goto_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.preview_page = page.min(self.page_count().saturating_sub(1));
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(crate) fn save_sheet(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        if ids.is_empty() {
            self.set_status("No photos in scope");
            cx.notify();
            return;
        }
        let name = if self.project.sheet.title.trim().is_empty() {
            format!("Sheet {:02}", self.project.saved_sheets.len() + 1)
        } else {
            self.project.sheet.title.trim().to_string()
        };
        self.project
            .upsert_saved_sheet(name.clone(), self.project.sheet.clone(), ids.clone());
        self.dirty = true;
        self.set_status(format!("Saved sheet / {name} / {} photos", ids.len()));
        self.log(format!("Sheet saved / {name} / {} photo(s)", ids.len()));
        cx.notify();
    }

    pub(crate) fn delete_sheet(&mut self, id: SheetId, cx: &mut Context<Self>) {
        let before = self.project.saved_sheets.len();
        self.project.saved_sheets.retain(|sheet| sheet.id != id);
        if self.project.saved_sheets.len() != before {
            self.dirty = true;
            self.log("Sheet deleted");
            self.set_status("Sheet deleted / project unsaved");
        }
        cx.notify();
    }

    pub(crate) fn load_sheet(&mut self, id: SheetId, cx: &mut Context<Self>) {
        let Some(sheet) = self
            .project
            .saved_sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .cloned()
        else {
            return;
        };
        self.project.sheet = sheet.settings.clone();
        if self.project.sheet.title.trim().is_empty() {
            self.project.sheet.title = sheet.name.clone();
        }
        self.selected = sheet
            .photo_ids
            .iter()
            .copied()
            .filter(|id| self.project.photo(*id).is_some())
            .collect();
        self.preview_page = 0;
        self.title_field
            .update(cx, |field, cx| field.set_value(sheet.name.clone(), cx));
        self.dirty = true;
        self.set_status(format!("Loaded sheet / {}", sheet.name));
        self.log(format!("Sheet loaded / {}", sheet.name));
        self.refresh_preview(cx);
        cx.notify();
    }

    // ----- project files -------------------------------------------------

    fn default_directory(&self) -> PathBuf {
        self.project_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn suggested_file_name(&self, extension: &str) -> String {
        let base = if self.project.name.trim().is_empty() {
            "contact-sheet".to_string()
        } else {
            self.project.name.clone()
        };
        format!("{}.{extension}", slug(&base))
    }

    pub(crate) fn open_project_prompt(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Open contact sheet".into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.open_project(path, cx));
        })
        .detach();
    }

    pub(crate) fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match Project::load(&path) {
            Ok(project) => {
                self.generation += 1;
                self.import_queue.clear();
                self.importing_paths.clear();
                self.imports_in_flight = 0;
                self.import_state = None;
                self.thumbs.clear();
                self.thumbs_queued.clear();
                self.rotated.clear();
                self.thumb_queue.clear();
                self.rotate_queue.clear();
                self.rotate_queued.clear();
                self.thumbs_in_flight = 0;
                self.rotate_in_flight = 0;
                self.selected = project.photo_ids().into_iter().collect();
                self.filter = Filter::All;
                self.search.clear();
                self.search_field
                    .update(cx, |field, cx| field.set_value("", cx));
                self.title_field.update(cx, |field, cx| {
                    field.set_value(project.sheet.title.clone(), cx)
                });
                let missing = project
                    .photos
                    .iter()
                    .filter(|photo| !photo.path.exists())
                    .count();
                self.project = project;
                self.project_path = Some(path.clone());
                self.dirty = false;
                self.preview_page = 0;
                self.rename_pending = None;
                self.log(format!("Opened {}", path.display()));
                self.set_status(format!(
                    "Opened {} photos{}",
                    self.project.photos.len(),
                    if missing > 0 {
                        format!(" / {missing} missing")
                    } else {
                        String::new()
                    }
                ));
                self.schedule_thumbs(cx);
                self.refresh_preview(cx);
                cx.notify();
            }
            Err(error) => {
                self.log(format!("Open failed: {error}"));
                self.set_status(format!("Open failed / {error}"));
                cx.notify();
            }
        }
    }

    pub(crate) fn save_project(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.project_path.clone() {
            self.write_project(path, cx);
        } else {
            self.save_project_as(cx);
        }
    }

    fn save_project_as(&mut self, cx: &mut Context<Self>) {
        let directory = self.default_directory();
        let suggested = self.suggested_file_name("contactsheet");
        cx.spawn(async move |this, cx| {
            let Ok(receiver) =
                cx.update(|app| app.prompt_for_new_path(&directory, Some(&suggested)))
            else {
                return;
            };
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let path = ensure_extension(path, "contactsheet");
            let _ = this.update(cx, |this, cx| this.write_project(path, cx));
        })
        .detach();
    }

    fn write_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.project.save(&path) {
            Ok(()) => {
                self.project_path = Some(path.clone());
                self.dirty = false;
                self.log(format!("Saved {}", path.display()));
                self.set_status(format!("Saved / {}", path.display()));
            }
            Err(error) => {
                self.log(format!("Save failed: {error}"));
                self.set_status(format!("Save failed / {error}"));
            }
        }
        cx.notify();
    }

    // ----- import --------------------------------------------------------

    pub(crate) fn import_prompt(&mut self, directories: bool, cx: &mut Context<Self>) {
        let prompt = if directories {
            "Import folder"
        } else {
            "Import photos"
        };
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: !directories,
                    directories,
                    multiple: true,
                    prompt: Some(prompt.into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.begin_import(paths, cx));
        })
        .detach();
    }

    pub(crate) fn begin_import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let generation = self.generation;
        self.set_status("Scanning…");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { import::discover_images(&paths) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.enqueue_import(found, cx);
            });
        })
        .detach();
    }

    fn enqueue_import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let existing: HashSet<PathBuf> = self
            .project
            .photos
            .iter()
            .map(|photo| photo.path.clone())
            .collect();
        let new_paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| !existing.contains(path) && !self.importing_paths.contains(path))
            .collect();

        if new_paths.is_empty() {
            self.set_status("No new photos found");
            self.log("Import / 0 new files");
            cx.notify();
            return;
        }

        if is_untitled(&self.project.name)
            && let Some(name) = new_paths[0].parent().and_then(|parent| parent.file_name())
        {
            self.project.name = name.to_string_lossy().into_owned();
        }

        if self.import_state.is_none() {
            self.import_state = Some(ImportState::default());
        }
        if let Some(state) = &mut self.import_state {
            state.total += new_paths.len();
        }
        self.importing_paths.extend(new_paths.iter().cloned());
        self.import_queue.extend(new_paths.iter().cloned());
        self.log(format!("Import / {} new files", new_paths.len()));
        self.set_status(format!(
            "Importing {}/{}",
            self.import_state
                .as_ref()
                .map(|state| state.done)
                .unwrap_or(0),
            self.import_state
                .as_ref()
                .map(|state| state.total)
                .unwrap_or(0)
        ));
        self.pump_import(cx);
        cx.notify();
    }

    fn pump_import(&mut self, cx: &mut Context<Self>) {
        while self.imports_in_flight < 3 {
            let Some(path) = self.import_queue.pop_front() else {
                break;
            };
            self.imports_in_flight += 1;
            let generation = self.generation;
            let task_path = path.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move { import::probe(&task_path, 320) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.imports_in_flight = this.imports_in_flight.saturating_sub(1);
                    this.accept_import(path, result);
                    if this.import_queue.is_empty() && this.imports_in_flight == 0 {
                        this.finish_import(cx);
                    } else {
                        this.pump_import(cx);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    fn accept_import(&mut self, path: PathBuf, result: anyhow::Result<Probe>) {
        let id = match result {
            Ok(probe) => {
                let id =
                    self.project
                        .add_photo(path.clone(), probe.width, probe.height, probe.bytes);
                self.thumbs
                    .insert(id, ThumbState::Ready(render_image(probe.preview)));
                id
            }
            Err(error) => {
                let id = self
                    .project
                    .add_photo(path.clone(), 0, 0, import::file_size(&path));
                if let Some(state) = &mut self.import_state
                    && state.errors.len() < 4
                {
                    state.errors.push(format!(
                        "{}: {error}",
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ));
                }
                self.thumbs
                    .insert(id, ThumbState::Failed(error.to_string()));
                id
            }
        };

        if let Some(state) = &mut self.import_state {
            state.done += 1;
        }
        self.importing_paths.remove(&path);
        self.selected.insert(id);
        self.dirty = true;
    }

    fn finish_import(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.import_state.take() {
            let errors = state.errors.len();
            for error in state.errors {
                self.log(format!("Import skip / {error}"));
            }
            self.log(format!("Import / {} files / {} errors", state.done, errors));
            self.set_status(format!(
                "Imported {} photos{}{}",
                state.done,
                if errors > 0 {
                    format!(" / {errors} could not be previewed")
                } else {
                    String::new()
                },
                if self.dirty { " / unsaved" } else { "" }
            ));
        }
        self.import_queue.clear();
        self.importing_paths.clear();
        self.imports_in_flight = 0;
        self.refresh_preview(cx);
        cx.notify();
    }

    // ----- thumbnails ----------------------------------------------------

    pub(crate) fn schedule_thumbs(&mut self, cx: &mut Context<Self>) {
        for photo in &self.project.photos {
            if self.thumbs.contains_key(&photo.id) {
                continue;
            }
            if self.thumbs_queued.contains(&photo.id) {
                continue;
            }
            self.thumbs_queued.insert(photo.id);
            self.thumb_queue.push_back((photo.id, photo.path.clone()));
        }
        self.pump_thumbs(cx);
    }

    fn pump_thumbs(&mut self, cx: &mut Context<Self>) {
        while self.thumbs_in_flight < 3 {
            let Some((id, path)) = self.thumb_queue.pop_front() else {
                break;
            };
            self.thumbs_in_flight += 1;
            let generation = self.generation;
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move { import::preview(&path, 320) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.thumbs_in_flight = this.thumbs_in_flight.saturating_sub(1);
                    this.thumbs_queued.remove(&id);
                    match result {
                        Ok(preview) => {
                            this.thumbs
                                .insert(id, ThumbState::Ready(render_image(preview)));
                        }
                        Err(error) => {
                            this.thumbs
                                .insert(id, ThumbState::Failed(error.to_string()));
                        }
                    }
                    this.pump_thumbs(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn schedule_preview_rotations(&mut self, cx: &mut Context<Self>) {
        if !self.project.sheet.rotate {
            return;
        }
        let photos = self.export_photos();
        if photos.is_empty() {
            return;
        }
        let sheet_layout = SheetLayout::from_settings(&self.project.sheet);
        let page = layout::plan_page(
            &self.project.sheet,
            &sheet_layout,
            &photos,
            self.current_page(),
        );
        for placement in &page.placements {
            if !placement.rotated {
                continue;
            }
            let Some(photo) = photos.get(placement.photo_index) else {
                continue;
            };
            if self.rotated.contains_key(&photo.id) || self.rotate_queued.contains(&photo.id) {
                continue;
            }
            self.rotate_queued.insert(photo.id);
            self.rotate_queue.push_back((photo.id, photo.path.clone()));
        }
        self.pump_rotations(cx);
    }

    fn pump_rotations(&mut self, cx: &mut Context<Self>) {
        while self.rotate_in_flight < 2 {
            let Some((id, path)) = self.rotate_queue.pop_front() else {
                break;
            };
            self.rotate_in_flight += 1;
            let generation = self.generation;
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move { import::preview(&path, 320) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.rotate_in_flight = this.rotate_in_flight.saturating_sub(1);
                    this.rotate_queued.remove(&id);
                    if this.project.photo(id).is_some()
                        && let Ok(preview) = result
                    {
                        let rotated = preview.rotated_clockwise();
                        if this.rotated.len() >= 256 {
                            this.rotated.clear();
                        }
                        this.rotated.insert(id, render_image(rotated));
                    }
                    this.pump_rotations(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(crate) fn thumb_image(&self, id: PhotoId) -> Option<Arc<RenderImage>> {
        match self.thumbs.get(&id) {
            Some(ThumbState::Ready(image)) => Some(image.clone()),
            _ => None,
        }
    }

    pub(crate) fn thumb_failed(&self, id: PhotoId) -> Option<&str> {
        match self.thumbs.get(&id) {
            Some(ThumbState::Failed(reason)) => Some(reason.as_str()),
            _ => None,
        }
    }

    pub(crate) fn preview_image(&self, id: PhotoId, rotated: bool) -> Option<Arc<RenderImage>> {
        if rotated && let Some(image) = self.rotated.get(&id) {
            return Some(image.clone());
        }
        self.thumb_image(id)
    }

    // ----- export --------------------------------------------------------

    pub(crate) fn export_current(&mut self, cx: &mut Context<Self>) {
        let photos = self.export_photos();
        if photos.is_empty() {
            self.set_status("No photos to export");
            cx.notify();
            return;
        }
        let title = self.project.sheet.title_or(&self.project.name).to_string();
        self.begin_export(title.clone(), self.project.sheet.clone(), photos, cx);
    }

    pub(crate) fn export_saved(&mut self, id: SheetId, cx: &mut Context<Self>) {
        let Some(sheet) = self
            .project
            .saved_sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .cloned()
        else {
            return;
        };
        let photos: Vec<Photo> = sheet
            .photo_ids
            .iter()
            .filter_map(|id| self.project.photo(*id).cloned())
            .collect();
        if photos.is_empty() {
            self.set_status(format!("Sheet {} has no photos", sheet.name));
            cx.notify();
            return;
        }
        self.begin_export(sheet.name.clone(), sheet.settings, photos, cx);
    }

    fn begin_export(
        &mut self,
        title: String,
        settings: SheetSettings,
        photos: Vec<Photo>,
        cx: &mut Context<Self>,
    ) {
        if self.exporting.is_some() {
            self.set_status("Export already in progress");
            cx.notify();
            return;
        }
        let directory = self.default_directory();
        let suggested = format!("{}.pdf", slug(&title));
        let project_name = self.project.name.clone();
        cx.spawn(async move |this, cx| {
            let Ok(receiver) =
                cx.update(|app| app.prompt_for_new_path(&directory, Some(&suggested)))
            else {
                return;
            };
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let output = ensure_extension(path, "pdf");
            let _ = this.update(cx, |this, cx| {
                this.exporting = Some(output.display().to_string());
                this.set_status(format!("Exporting / {}", output.display()));
                this.log(format!("Export start / {} photos", photos.len()));
                cx.notify();
            });

            let export_settings = settings.clone();
            let export_output = output.clone();
            let result = cx
                .background_spawn(async move {
                    pdf::export_pdf(&project_name, &export_settings, &photos, &export_output)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.exporting = None;
                match result {
                    Ok(summary) => this.export_finished(&title, summary),
                    Err(error) => {
                        this.log(format!("Export failed: {error}"));
                        this.set_status(format!("Export failed / {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn export_finished(&mut self, title: &str, summary: ExportSummary) {
        let kilobytes = summary.bytes / 1024;
        let missing = summary.missing.len();
        self.log(format!(
            "Export / {} / {} pages / {} KB / {} missing",
            title, summary.pages, kilobytes, missing
        ));
        for reason in summary.missing.iter().take(3) {
            self.log(format!("Export skip / {reason}"));
        }
        self.set_status(format!(
            "Exported {} pages / {} KB{}{}",
            summary.pages,
            kilobytes,
            if missing > 0 {
                format!(" / {missing} missing")
            } else {
                String::new()
            },
            if self.dirty { " / project unsaved" } else { "" }
        ));
        reveal(&summary.path);
    }
}

impl Focusable for ContactSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ContactSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            window.focus(&self.focus);
            self.focused_once = true;
        }
        if !self.appearance_initialized {
            self.dark = matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            );
            self.appearance_initialized = true;
        }
        theme::set_dark(self.dark);
        self.clamp_panel_widths(window);

        let title = format!(
            "{}{} — Contact sheet",
            self.project.name,
            if self.dirty { " *" } else { "" }
        );
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }

        let overlay = self.render_overlay(cx);

        div()
            .key_context("ContactSheet")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .on_action(cx.listener(|this, _: &SaveProject, _, cx| this.save_project(cx)))
            .on_action(cx.listener(|this, _: &OpenProject, _, cx| this.open_project_prompt(cx)))
            .on_action(cx.listener(|this, _: &ExportSheet, _, cx| this.export_current(cx)))
            .on_action(cx.listener(|this, _: &ImportFiles, _, cx| this.import_prompt(false, cx)))
            .on_action(cx.listener(|this, _: &ImportFolder, _, cx| this.import_prompt(true, cx)))
            .on_action(cx.listener(|this, _: &SelectAllPhotos, _, cx| this.select_all_visible(cx)))
            .on_action(cx.listener(|this, _: &DeselectPhotos, _, cx| this.deselect_all(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.rename_pending.is_some() {
                    this.cancel_rename(cx);
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.begin_import(paths.paths().to_vec(), cx)
            }))
            .on_mouse_move(cx.listener(Self::resize_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_resize))
            .child(self.render_topbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_sidebar(window, cx))
                    .child(divider(
                        "resize-left",
                        ResizeTarget::Left,
                        self.resizing == Some(ResizeTarget::Left),
                        cx,
                    ))
                    .child(self.render_library(window, cx))
                    .child(divider(
                        "resize-right",
                        ResizeTarget::Right,
                        self.resizing == Some(ResizeTarget::Right),
                        cx,
                    ))
                    .child(self.render_inspector(window, cx)),
            )
            .child(self.render_statusbar())
            .children(overlay)
    }
}

fn divider(
    id: &'static str,
    target: ResizeTarget,
    active: bool,
    cx: &mut Context<ContactSheet>,
) -> AnyElement {
    let line = if active { theme::ink() } else { theme::rule() };
    div()
        .id(id)
        .w(px(6.0))
        .h_full()
        .flex_shrink_0()
        .cursor(CursorStyle::ResizeColumn)
        .hover(|style| style.bg(theme::accent_soft()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| this.begin_resize(target, cx)),
        )
        .child(div().w(px(1.0)).h_full().bg(line))
        .into_any_element()
}

fn render_image(preview: PreviewImage) -> Arc<RenderImage> {
    let width = preview.width.max(1);
    let height = preview.height.max(1);
    let bgra = preview.to_bgra();
    let buffer = RgbaImage::from_raw(width, height, bgra).unwrap_or_else(|| RgbaImage::new(1, 1));
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}

fn is_untitled(name: &str) -> bool {
    let name = name.trim();
    name.is_empty() || name.eq_ignore_ascii_case("untitled")
}

fn ensure_extension(path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension(extension)
    }
}

pub(crate) fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character);
            last_dash = false;
        } else if !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "contact-sheet".to_string()
    } else {
        output
    }
}

fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slugs_are_flat_and_safe() {
        assert_eq!(slug("Street / Walk"), "street-walk");
        assert_eq!(slug("  "), "contact-sheet");
        assert_eq!(slug("A--B"), "a-b");
    }
}
