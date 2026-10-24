//! Application state and workflows. Panels live in `library`, `sidebar`,
//! `inspector` and `overlay`; this module owns the data and the async bridges to
//! native dialogs and the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    App, Context, ExternalPaths, FocusHandle, Focusable, PathPromptOptions, Render, RenderImage,
    Window, actions, div, prelude::*, px,
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
            TextField::new(cx, "FILTER FILE NAMES").on_change(move |value, app| {
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
            TextField::new(cx, "SHEET TITLE").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.project.sheet.title = value.to_string();
                    this.dirty = true;
                    cx.notify();
                });
            })
        });

        let prefix_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "PREFIX").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.rename_options.prefix = value.to_string();
                    cx.notify();
                });
            })
        });

        let suffix_field = cx.new(|cx| {
            let change_weak = weak.clone();
            let submit_weak = weak.clone();
            TextField::new(cx, "SUFFIX")
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
            TextField::new(cx, "TAG, E.G. STREET").on_submit(move |value, app| {
                let value = value.to_string();
                let _ = weak.update(app, |this, cx| this.apply_tag_value(value, cx, true));
            })
        });

        let mut log = VecDeque::new();
        log.push_back("SESSION READY".to_string());

        Self {
            project: Project::new("UNTITLED"),
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
            status: "IMPORT FILES OR A FOLDER TO BEGIN".to_string(),
            log,
            focus: cx.focus_handle(),
            focused_once: false,
            window_title: String::new(),
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
            Filter::All => "ALL IMAGES".to_string(),
            Filter::Untagged => "UNTAGGED".to_string(),
            Filter::Tag(tag) => format!("#{tag}"),
        }
    }

    pub(crate) fn summary(&self) -> String {
        let photos = self.working_count();
        let layout = SheetLayout::from_settings(&self.project.sheet);
        let pages = self.page_count();
        format!(
            "{photos} PHOTOS / {pages} PAGE{} / {} UP",
            if pages == 1 { "" } else { "S" },
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
            self.set_status("SELECT PHOTOS TO REMOVE FROM THE PROJECT");
            cx.notify();
            return;
        }
        let ids = self.target_ids();
        if ids.is_empty() {
            self.set_status("SELECT PHOTOS FIRST");
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
        self.log(format!("REMOVED {removed} PHOTOS FROM PROJECT"));
        self.set_status(format!("REMOVED {removed} FROM PROJECT / FILES UNTOUCHED"));
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
            self.set_status("TYPE A TAG FIRST");
            cx.notify();
            return;
        }
        if ids.is_empty() {
            self.set_status("NO PHOTOS IN SCOPE");
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
        self.set_status(format!("TAGGED {changed} / {tag} / {} IN SCOPE", ids.len()));
        self.log(format!("TAG +{tag} / {changed} PHOTO(S)"));
        cx.notify();
    }

    pub(crate) fn remove_tag(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        let tag = self.tag_field.read(cx).value().trim().to_string();
        if tag.is_empty() {
            self.set_status("TYPE THE TAG TO REMOVE");
            cx.notify();
            return;
        }
        let changed = batch::remove_tag(&mut self.project, &ids, &tag);
        if changed > 0 {
            self.dirty = true;
        }
        self.tag_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.set_status(format!("REMOVED TAG {tag} FROM {changed} PHOTO(S)"));
        self.log(format!("TAG -{tag} / {changed} PHOTO(S)"));
        cx.notify();
    }

    pub(crate) fn clear_tags(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        let changed = batch::clear_tags(&mut self.project, &ids);
        if changed > 0 {
            self.dirty = true;
        }
        self.set_status(format!("CLEARED TAGS ON {changed} PHOTO(S)"));
        self.log(format!("TAGS CLEARED / {changed} PHOTO(S)"));
        self.refresh_preview(cx);
        cx.notify();
    }

    // ----- rename --------------------------------------------------------

    pub(crate) fn preview_rename(&mut self, cx: &mut Context<Self>) {
        let ids = self.target_ids();
        if ids.is_empty() {
            self.set_status("SELECT PHOTOS TO RENAME");
            cx.notify();
            return;
        }
        let plan = batch::plan_rename(&self.project, &ids, &self.rename_options);
        if plan.entries.is_empty() {
            self.set_status("NO CHANGES / NAMES ALREADY MATCH");
            self.log(format!("RENAME: 0 CHANGES / {} SKIPPED", plan.skipped));
            cx.notify();
            return;
        }
        let conflicts = plan.conflicts().count();
        if conflicts > 0 {
            self.set_status(format!("{conflicts} RENAME CONFLICT(S)"));
        } else {
            self.set_status(format!("RENAME READY / {} FILES", plan.entries.len()));
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
            self.set_status("FIX RENAME CONFLICTS FIRST");
            cx.notify();
            return;
        }
        let report = batch::apply_rename(&mut self.project, &plan);
        if report.renamed_count() > 0 {
            self.dirty = true;
        }
        for (_, error) in &report.errors {
            self.log(format!("RENAME ERROR: {error}"));
        }
        if report.renamed_count() > 0 {
            if let Some(path) = self.project_path.clone() {
                self.write_project(path, cx);
            } else {
                self.log("RENAME APPLIED / PROJECT UNSAVED");
            }
        }
        self.set_status(format!(
            "RENAMED {} FILE(S){}",
            report.renamed_count(),
            if report.errors.is_empty() {
                String::new()
            } else {
                format!(" / {} ERROR(S)", report.errors.len())
            }
        ));
        self.log(format!(
            "RENAME / {} OK / {} ERROR",
            report.renamed_count(),
            report.errors.len()
        ));
        cx.notify();
    }

    pub(crate) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename_pending = None;
        self.set_status("RENAME CANCELLED");
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
            self.set_status("NO PHOTOS IN SCOPE");
            cx.notify();
            return;
        }
        let name = if self.project.sheet.title.trim().is_empty() {
            format!("SHEET {:02}", self.project.saved_sheets.len() + 1)
        } else {
            self.project.sheet.title.trim().to_string()
        };
        self.project
            .upsert_saved_sheet(name.clone(), self.project.sheet.clone(), ids.clone());
        self.dirty = true;
        self.set_status(format!("SAVED SHEET / {name} / {} PHOTOS", ids.len()));
        self.log(format!("SHEET SAVED / {name} / {} PHOTO(S)", ids.len()));
        cx.notify();
    }

    pub(crate) fn delete_sheet(&mut self, id: SheetId, cx: &mut Context<Self>) {
        let before = self.project.saved_sheets.len();
        self.project.saved_sheets.retain(|sheet| sheet.id != id);
        if self.project.saved_sheets.len() != before {
            self.dirty = true;
            self.log("SHEET DELETED");
            self.set_status("SHEET DELETED / PROJECT UNSAVED");
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
        self.set_status(format!("LOADED SHEET / {}", sheet.name));
        self.log(format!("SHEET LOADED / {}", sheet.name));
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
                self.log(format!("OPENED {}", path.display()));
                self.set_status(format!(
                    "OPENED {} PHOTOS{}",
                    self.project.photos.len(),
                    if missing > 0 {
                        format!(" / {missing} MISSING")
                    } else {
                        String::new()
                    }
                ));
                self.schedule_thumbs(cx);
                self.refresh_preview(cx);
                cx.notify();
            }
            Err(error) => {
                self.log(format!("OPEN FAILED: {error}"));
                self.set_status(format!("OPEN FAILED / {error}"));
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
                self.log(format!("SAVED {}", path.display()));
                self.set_status(format!("SAVED / {}", path.display()));
            }
            Err(error) => {
                self.log(format!("SAVE FAILED: {error}"));
                self.set_status(format!("SAVE FAILED / {error}"));
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
        self.set_status("SCANNING…");
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
            self.set_status("NO NEW PHOTOS FOUND");
            self.log("IMPORT / 0 NEW FILES");
            cx.notify();
            return;
        }

        if self.project.name == "UNTITLED"
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
        self.log(format!("IMPORT / {} NEW FILES", new_paths.len()));
        self.set_status(format!(
            "IMPORTING {}/{}",
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
                self.log(format!("IMPORT SKIP / {error}"));
            }
            self.log(format!("IMPORT / {} FILES / {} ERRORS", state.done, errors));
            self.set_status(format!(
                "IMPORTED {} PHOTOS{}{}",
                state.done,
                if errors > 0 {
                    format!(" / {errors} COULD NOT BE PREVIEWED")
                } else {
                    String::new()
                },
                if self.dirty { " / UNSAVED" } else { "" }
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
            self.set_status("NO PHOTOS TO EXPORT");
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
            self.set_status(format!("SHEET {} HAS NO PHOTOS", sheet.name));
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
            self.set_status("EXPORT ALREADY IN PROGRESS");
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
                this.set_status(format!("EXPORTING / {}", output.display()));
                this.log(format!("EXPORT START / {} PHOTOS", photos.len()));
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
                        this.log(format!("EXPORT FAILED: {error}"));
                        this.set_status(format!("EXPORT FAILED / {error}"));
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
            "EXPORT / {} / {} PAGES / {} KB / {} MISSING",
            title, summary.pages, kilobytes, missing
        ));
        for reason in summary.missing.iter().take(3) {
            self.log(format!("EXPORT SKIP / {reason}"));
        }
        self.set_status(format!(
            "EXPORTED {} PAGES / {} KB{}{}",
            summary.pages,
            kilobytes,
            if missing > 0 {
                format!(" / {missing} MISSING")
            } else {
                String::new()
            },
            if self.dirty { " / PROJECT UNSAVED" } else { "" }
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

        let title = format!(
            "{}{} — Slate",
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
            .child(self.render_topbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_sidebar(window, cx))
                    .child(self.render_library(window, cx))
                    .child(self.render_inspector(window, cx)),
            )
            .child(self.render_statusbar())
            .children(overlay)
    }
}

fn render_image(preview: PreviewImage) -> Arc<RenderImage> {
    let width = preview.width.max(1);
    let height = preview.height.max(1);
    let bgra = preview.to_bgra();
    let buffer = RgbaImage::from_raw(width, height, bgra).unwrap_or_else(|| RgbaImage::new(1, 1));
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
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
