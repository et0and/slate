//! Application state and workflows. Panels live in `library`, `sidebar`,
//! `inspector` and `overlay`; this module owns the data and the async bridges to
//! native dialogs and the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, CursorStyle, ExternalPaths, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, PathPromptOptions, Render, RenderImage, UniformListScrollHandle,
    Window, WindowAppearance, actions, div, prelude::*, px,
};
use image::{Frame, RgbaImage};
use slate_core::PreviewImage;
use slate_core::batch::{self, RenameOptions, RenamePlan};
use slate_core::import::{self, Probe};
use slate_core::model::{
    Filter, Paper, Photo, PhotoId, Project, QuarterTurns, SheetId, SheetSettings,
};
use slate_core::pdf::{self, ExportSummary};
use slate_core::store::{LocalStore, RecentProject};

use crate::drive;
use crate::drive_ui::{DriveDownload, DriveMode, DriveNotice, DrivePicker};
use crate::history::{History, Snapshot};
use crate::memory;
use crate::panels::{DIVIDER_W, PanelLayout, ResizeTarget};
use crate::text_field::TextField;
use crate::theme;
use crate::view::ViewState;

actions!(
    contact_sheet_actions,
    [
        Quit,
        NewProject,
        OpenProject,
        SaveProject,
        SaveAs,
        ExportSheet,
        ImportFiles,
        ImportFolder,
        SelectAllPhotos,
        DeselectPhotos,
        Undo,
        Redo,
        Dismiss,
        SaveSheet,
        InvertSelection,
        RemoveSelected,
        ResetPanels,
        About,
        ImportDrive,
        ExportDrive,
        LinkDrive,
        UnlinkDrive,
        HideSlate,
        HideOthers,
        OpenWebsite,
        ReportIssue,
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json, namespace = contact_sheet)]
pub(crate) struct OpenRecent {
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json, namespace = contact_sheet)]
pub(crate) struct SetTheme {
    pub theme: theme::Theme,
}

const THUMB_CACHE: usize = 32;
const THUMB_EDGE: u32 = 224;
const THUMB_QUEUE_LIMIT: usize = 512;

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

    pub(crate) selected: Arc<HashSet<PhotoId>>,
    pub(crate) filter: Filter,
    pub(crate) search: String,
    pub(crate) view: ViewState,

    pub(crate) thumbs: HashMap<PhotoId, ThumbState>,
    pub(crate) thumb_queue: VecDeque<(PhotoId, PathBuf)>,
    pub(crate) thumbs_in_flight: usize,
    pub(crate) thumbs_queued: HashSet<PhotoId>,
    pub(crate) thumb_order: VecDeque<PhotoId>,
    pub(crate) rotated_thumbs: HashMap<(PhotoId, QuarterTurns), Arc<RenderImage>>,
    pub(crate) grid_scroll: UniformListScrollHandle,
    pub(crate) rename_scroll: UniformListScrollHandle,

    pub(crate) import_queue: VecDeque<PathBuf>,
    pub(crate) importing_paths: HashSet<PathBuf>,
    pub(crate) imports_in_flight: usize,
    pub(crate) import_state: Option<ImportState>,
    pub(crate) last_import_render: Instant,

    pub(crate) preview_page: usize,
    pub(crate) preview_modal_open: bool,
    pub(crate) paper_menu_open: bool,
    pub(crate) rename_pending: Option<Arc<RenamePlan>>,
    pub(crate) rename_options: RenameOptions,
    pub(crate) exporting: Option<String>,

    pub(crate) status: String,
    pub(crate) log: VecDeque<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) focused_once: bool,
    pub(crate) theme: theme::Theme,
    pub(crate) appearance_initialized: bool,
    pub(crate) panels: PanelLayout,
    pub(crate) menu_open: Option<crate::menubar::MenuId>,
    pub(crate) about_open: bool,
    pub(crate) drive_account: Option<drive::DriveAccount>,
    pub(crate) drive_picker: Option<DrivePicker>,
    pub(crate) drive_notice: Option<DriveNotice>,
    pub(crate) drive_linking: bool,
    pub(crate) drive_download: Option<DriveDownload>,
    pub(crate) history: History,
    pub(crate) store: LocalStore,
    pub(crate) recent_projects: Vec<RecentProject>,

    pub(crate) search_field: gpui::Entity<TextField>,
    pub(crate) title_field: gpui::Entity<TextField>,
    pub(crate) custom_width_field: gpui::Entity<TextField>,
    pub(crate) custom_height_field: gpui::Entity<TextField>,
    pub(crate) prefix_field: gpui::Entity<TextField>,
    pub(crate) suffix_field: gpui::Entity<TextField>,
    pub(crate) tag_field: gpui::Entity<TextField>,
}

impl ContactSheet {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();

        let search_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "File name").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.search = value.to_string();
                    this.preview_page = 0;
                    this.filter_changed();
                    this.refresh_preview();
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
            TextField::new(cx, "walk-").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| {
                    this.rename_options.prefix = value.to_string();
                    cx.notify();
                });
            })
        });

        let suffix_field = cx.new(|cx| {
            let change_weak = weak.clone();
            let submit_weak = weak.clone();
            TextField::new(cx, "-web")
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

        let custom_width_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Width").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| this.set_custom_paper_width(value, cx));
            })
        });

        let custom_height_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Height").on_change(move |value, app| {
                let _ = weak.update(app, |this, cx| this.set_custom_paper_height(value, cx));
            })
        });

        let mut log = VecDeque::new();
        log.push_back("Session ready".to_string());
        let store = LocalStore::default_location();
        let recent_projects = store.recent_projects(5).unwrap_or_default();

        let mut this = Self {
            project: Project::new("Untitled"),
            project_path: None,
            dirty: false,
            generation: 1,
            selected: Arc::new(HashSet::new()),
            filter: Filter::All,
            search: String::new(),
            view: ViewState::new(),
            thumbs: HashMap::new(),
            thumb_queue: VecDeque::new(),
            thumbs_in_flight: 0,
            thumbs_queued: HashSet::new(),
            thumb_order: VecDeque::new(),
            rotated_thumbs: HashMap::new(),
            grid_scroll: UniformListScrollHandle::default(),
            rename_scroll: UniformListScrollHandle::default(),
            import_queue: VecDeque::new(),
            importing_paths: HashSet::new(),
            imports_in_flight: 0,
            import_state: None,
            last_import_render: Instant::now(),
            preview_page: 0,
            preview_modal_open: false,
            paper_menu_open: false,
            rename_pending: None,
            rename_options: RenameOptions::default(),
            exporting: None,
            status: "Ready".to_string(),
            log,
            focus: cx.focus_handle(),
            focused_once: false,
            theme: theme::Theme::Light,
            appearance_initialized: false,
            panels: PanelLayout::default(),
            menu_open: None,
            about_open: false,
            drive_account: drive::load_account(),
            drive_picker: None,
            drive_notice: None,
            drive_linking: false,
            drive_download: None,
            history: History::default(),
            store,
            recent_projects,
            search_field,
            title_field,
            custom_width_field,
            custom_height_field,
            prefix_field,
            suffix_field,
            tag_field,
        };
        this.sync_paper_fields(cx);
        this.sync_native_menus(cx);
        this
    }

    // ----- queries -------------------------------------------------------

    fn ensure_view(&mut self) {
        let view = &mut self.view;
        view.ensure(&self.project, &self.filter, &self.search, &self.selected);
    }

    pub(crate) fn project_changed(&mut self) {
        self.view.invalidate_project();
    }

    pub(crate) fn selection_changed(&mut self) {
        self.view.invalidate_selection();
    }

    pub(crate) fn filter_changed(&mut self) {
        self.view.invalidate_filter();
    }

    pub(crate) fn visible_indices(&mut self) -> Arc<Vec<usize>> {
        self.ensure_view();
        self.view.visible()
    }

    pub(crate) fn working_count(&mut self) -> usize {
        self.scope_ids().len()
    }

    pub(crate) fn page_count(&mut self) -> usize {
        self.working_count()
            .div_ceil(self.project.sheet.per_page())
            .max(1)
    }

    pub(crate) fn current_page(&mut self) -> usize {
        self.preview_page.min(self.page_count().saturating_sub(1))
    }

    pub(crate) fn tag_counts(&mut self) -> Arc<std::collections::BTreeMap<String, usize>> {
        self.ensure_view();
        self.view.tags()
    }

    pub(crate) fn untagged_count(&mut self) -> usize {
        self.ensure_view();
        self.view.untagged()
    }

    /// Explicit selection wins. With no selection, every visible photo is the
    /// working set, so import -> export needs no extra step.
    pub(crate) fn scope_ids(&mut self) -> Arc<Vec<PhotoId>> {
        self.ensure_view();
        self.view.scope()
    }

    pub(crate) fn export_photos(&mut self) -> Vec<Photo> {
        self.scope_ids()
            .iter()
            .filter_map(|id| self.project.photo(*id).cloned())
            .collect()
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

    fn begin_resize(&mut self, target: ResizeTarget, cx: &mut Context<Self>) {
        self.panels.begin(target);
        cx.notify();
    }

    fn resize_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.panels.resizing() else {
            return;
        };
        let total = f32::from(window.viewport_size().width);
        let x = f32::from(event.position.x);
        self.panels.resize(target, x, total);
        cx.notify();
    }

    fn end_resize(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.panels.end() {
            cx.notify();
        }
    }

    pub(crate) fn set_theme(&mut self, theme: theme::Theme, cx: &mut Context<Self>) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        theme::set_theme(theme);
        theme::apply_native_appearance(theme);
        self.sync_native_menus(cx);
        let fields = [
            self.search_field.clone(),
            self.title_field.clone(),
            self.custom_width_field.clone(),
            self.custom_height_field.clone(),
            self.prefix_field.clone(),
            self.suffix_field.clone(),
            self.tag_field.clone(),
        ];
        for field in fields {
            field.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    pub(crate) fn refresh_preview(&mut self) {
        self.preview_page = self.preview_page.min(self.page_count().saturating_sub(1));
    }

    fn refresh_and_notify(&mut self, cx: &mut Context<Self>) {
        self.refresh_preview();
        cx.notify();
    }

    fn sync_paper_fields(&mut self, cx: &mut Context<Self>) {
        let width = paper_mm_text(self.project.sheet.custom_paper_width_mm);
        let height = paper_mm_text(self.project.sheet.custom_paper_height_mm);
        self.custom_width_field
            .update(cx, |field, cx| field.set_value(width, cx));
        self.custom_height_field
            .update(cx, |field, cx| field.set_value(height, cx));
    }

    pub(crate) fn toggle_paper_menu(&mut self, cx: &mut Context<Self>) {
        self.paper_menu_open = !self.paper_menu_open;
        cx.notify();
    }

    pub(crate) fn set_paper(&mut self, paper: Paper, cx: &mut Context<Self>) {
        self.paper_menu_open = false;
        if self.project.sheet.paper == paper {
            cx.notify();
            return;
        }
        self.checkpoint();
        self.project.sheet.paper = paper;
        self.sync_paper_fields(cx);
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn set_custom_paper_width(&mut self, value: &str, cx: &mut Context<Self>) {
        self.set_custom_paper_dimension(value, true, cx);
    }

    pub(crate) fn set_custom_paper_height(&mut self, value: &str, cx: &mut Context<Self>) {
        self.set_custom_paper_dimension(value, false, cx);
    }

    fn set_custom_paper_dimension(&mut self, value: &str, width: bool, cx: &mut Context<Self>) {
        let Ok(mm) = value.trim().parse::<f32>() else {
            return;
        };
        if !mm.is_finite() {
            return;
        }
        let mm = mm.clamp(50.0, 2000.0);
        if width {
            if self.project.sheet.custom_paper_width_mm == mm {
                return;
            }
            self.project.sheet.custom_paper_width_mm = mm;
        } else {
            if self.project.sheet.custom_paper_height_mm == mm {
                return;
            }
            self.project.sheet.custom_paper_height_mm = mm;
        }
        self.dirty = true;
        self.refresh_and_notify(cx);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            selected: Arc::clone(&self.selected),
            dirty: self.dirty,
        }
    }

    fn checkpoint(&mut self) {
        self.history.checkpoint(self.snapshot());
    }

    fn remember_current_project(&mut self) {
        let Some(path) = self.project_path.clone() else {
            return;
        };
        match self.store.remember_project(&path, &self.project) {
            Ok(()) => {
                self.recent_projects = self.store.recent_projects(5).unwrap_or_default();
            }
            Err(error) => self.log(format!("Catalog / {error}")),
        }
    }

    fn restore(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let photos_changed = !self
            .project
            .photos
            .shares_data_with(&snapshot.project.photos);
        self.generation = self.generation.wrapping_add(1);
        self.import_queue.clear();
        self.importing_paths.clear();
        self.imports_in_flight = 0;
        self.import_state = None;
        self.thumbs.clear();
        self.thumbs_queued.clear();
        self.thumb_order.clear();
        self.thumb_queue.clear();
        self.thumbs_in_flight = 0;
        self.rotated_thumbs.clear();
        self.grid_scroll = UniformListScrollHandle::default();
        self.project = snapshot.project;
        self.selected = snapshot.selected;
        self.dirty = snapshot.dirty;
        if photos_changed {
            self.project_changed();
        }
        self.selection_changed();
        self.preview_page = 0;
        self.rename_pending = None;
        self.title_field.update(cx, |field, cx| {
            field.set_value(self.project.sheet.title.clone(), cx);
        });
        self.sync_paper_fields(cx);
        self.refresh_and_notify(cx);
    }

    fn editing_text(&self, window: &Window, cx: &App) -> bool {
        [
            &self.search_field,
            &self.title_field,
            &self.custom_width_field,
            &self.custom_height_field,
            &self.prefix_field,
            &self.suffix_field,
            &self.tag_field,
        ]
        .iter()
        .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.about_open {
            return;
        }
        if self.rename_pending.is_some() {
            self.set_status("Finish or cancel the rename first");
        } else if let Some(previous) = self.history.undo(self.snapshot()) {
            self.restore(previous, cx);
            self.set_status("Undone");
            self.log("Undo");
        } else {
            self.set_status("Nothing to undo");
        }
        cx.notify();
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.about_open {
            return;
        }
        if self.rename_pending.is_some() {
            self.set_status("Finish or cancel the rename first");
        } else if let Some(next) = self.history.redo(self.snapshot()) {
            self.restore(next, cx);
            self.set_status("Redone");
            self.log("Redo");
        } else {
            self.set_status("Nothing to redo");
        }
        cx.notify();
    }

    // ----- selection -----------------------------------------------------

    pub(crate) fn toggle_selected(&mut self, id: PhotoId, cx: &mut Context<Self>) {
        let selected = Arc::make_mut(&mut self.selected);
        if !selected.remove(&id) {
            selected.insert(id);
        }
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn select_all_visible(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible_indices();
        let selected = Arc::make_mut(&mut self.selected);
        for index in visible.iter() {
            selected.insert(self.project.photos[*index].id());
        }
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filter = Filter::All;
        self.search.clear();
        self.search_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.filter_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn deselect_all(&mut self, cx: &mut Context<Self>) {
        self.selected = Arc::new(HashSet::new());
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn invert_visible(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible_indices();
        let selected = Arc::make_mut(&mut self.selected);
        for index in visible.iter() {
            let id = self.project.photos[*index].id();
            if !selected.remove(&id) {
                selected.insert(id);
            }
        }
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn remove_selected(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            self.set_status("Nothing selected");
            cx.notify();
            return;
        }
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("Nothing selected");
            cx.notify();
            return;
        }
        self.checkpoint();
        let removed = self.project.remove_photos(&ids);
        let removed_ids: HashSet<PhotoId> = ids.iter().copied().collect();
        let selected = Arc::make_mut(&mut self.selected);
        for id in removed_ids.iter() {
            selected.remove(id);
            self.thumbs.remove(id);
            self.thumbs_queued.remove(id);
        }
        self.thumb_order.retain(|id| !removed_ids.contains(id));
        self.rotated_thumbs
            .retain(|(photo_id, _), _| !removed_ids.contains(photo_id));
        self.project_changed();
        self.dirty = true;
        self.log(format!("Removed {removed} photos from the project"));
        self.set_status(format!(
            "Removed {removed} from the project / files untouched"
        ));
        self.refresh_and_notify(cx);
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
        let ids = self.scope_ids();
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
        self.checkpoint();
        let changed = batch::add_tag(&mut self.project, &ids, &tag);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
        } else {
            self.history.discard_last();
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
        let ids = self.scope_ids();
        let tag = self.tag_field.read(cx).value().trim().to_string();
        if tag.is_empty() {
            self.set_status("Type the tag to remove");
            cx.notify();
            return;
        }
        self.checkpoint();
        let changed = batch::remove_tag(&mut self.project, &ids, &tag);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
        } else {
            self.history.discard_last();
        }
        self.tag_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.set_status(format!("Removed tag {tag} from {changed} photo(s)"));
        self.log(format!("Tag -{tag} / {changed} photo(s)"));
        cx.notify();
    }

    pub(crate) fn clear_tags(&mut self, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        self.checkpoint();
        let changed = batch::clear_tags(&mut self.project, &ids);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
        } else {
            self.history.discard_last();
        }
        self.set_status(format!("Cleared tags on {changed} photo(s)"));
        self.log(format!("Tags cleared / {changed} photo(s)"));
        self.refresh_and_notify(cx);
    }

    // ----- rename --------------------------------------------------------

    pub(crate) fn preview_rename(&mut self, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
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
        let conflicts = plan.conflicts;
        if conflicts > 0 {
            self.set_status(format!("{conflicts} rename conflict(s)"));
        } else {
            self.set_status(format!("Rename ready / {} files", plan.entries.len()));
        }
        self.rename_scroll = UniformListScrollHandle::default();
        self.rename_pending = Some(Arc::new(plan));
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
            self.project_changed();
            self.dirty = true;
            self.history.clear();
            self.log("History cleared after rename");
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
        self.refresh_and_notify(cx);
    }

    pub(crate) fn toggle_orientation(&mut self, cx: &mut Context<Self>) {
        self.checkpoint();
        self.project.sheet.orientation = self.project.sheet.orientation.toggled();
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_columns(&mut self, delta: i8, cx: &mut Context<Self>) {
        self.checkpoint();
        let value = (i16::from(self.project.sheet.columns) + i16::from(delta)).clamp(1, 12);
        self.project.sheet.columns = value as u8;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_rows(&mut self, delta: i8, cx: &mut Context<Self>) {
        self.checkpoint();
        let value = (i16::from(self.project.sheet.rows) + i16::from(delta)).clamp(1, 12);
        self.project.sheet.rows = value as u8;
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_margin(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.checkpoint();
        self.project.sheet.margin_mm = (self.project.sheet.margin_mm + delta).clamp(2.0, 40.0);
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn nudge_gutter(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.checkpoint();
        self.project.sheet.gutter_mm = (self.project.sheet.gutter_mm + delta).clamp(0.0, 20.0);
        self.mark_sheet_changed(cx);
    }

    fn update_sheet(&mut self, update: impl FnOnce(&mut SheetSettings), cx: &mut Context<Self>) {
        self.checkpoint();
        update(&mut self.project.sheet);
        self.mark_sheet_changed(cx);
    }

    pub(crate) fn toggle_captions(&mut self, cx: &mut Context<Self>) {
        self.update_sheet(|sheet| sheet.captions = !sheet.captions, cx);
    }

    pub(crate) fn toggle_index(&mut self, cx: &mut Context<Self>) {
        self.update_sheet(|sheet| sheet.index = !sheet.index, cx);
    }

    pub(crate) fn toggle_rotate(&mut self, cx: &mut Context<Self>) {
        self.update_sheet(|sheet| sheet.rotate = !sheet.rotate, cx);
    }

    pub(crate) fn toggle_header(&mut self, cx: &mut Context<Self>) {
        self.update_sheet(|sheet| sheet.header = !sheet.header, cx);
    }

    pub(crate) fn toggle_footer(&mut self, cx: &mut Context<Self>) {
        self.update_sheet(|sheet| sheet.footer = !sheet.footer, cx);
    }

    pub(crate) fn goto_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.preview_page = page.min(self.page_count().saturating_sub(1));
        self.refresh_and_notify(cx);
    }

    pub(crate) fn save_sheet(&mut self, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("No photos in scope");
            cx.notify();
            return;
        }
        self.checkpoint();
        let name = if self.project.sheet.title.trim().is_empty() {
            format!("Sheet {:02}", self.project.saved_sheets.len() + 1)
        } else {
            self.project.sheet.title.trim().to_string()
        };
        self.project.upsert_saved_sheet(
            name.clone(),
            self.project.sheet.clone(),
            ids.as_ref().clone(),
        );
        self.dirty = true;
        self.set_status(format!("Layout written / {name} / {} photos", ids.len()));
        self.log(format!("Sheet saved / {name} / {} photo(s)", ids.len()));
        cx.notify();
    }

    pub(crate) fn delete_sheet(&mut self, id: SheetId, cx: &mut Context<Self>) {
        let before = self.project.saved_sheets.len();
        self.checkpoint();
        self.project.saved_sheets.retain(|sheet| sheet.id != id);
        if self.project.saved_sheets.len() != before {
            self.dirty = true;
            self.log("Sheet deleted");
            self.set_status("Sheet deleted / project unsaved");
        } else {
            self.history.discard_last();
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
        self.checkpoint();
        self.project.sheet = sheet.settings.clone();
        if self.project.sheet.title.trim().is_empty() {
            self.project.sheet.title = sheet.name.clone();
        }
        self.selected = Arc::new(
            sheet
                .photo_ids
                .iter()
                .copied()
                .filter(|id| self.project.photo(*id).is_some())
                .collect(),
        );
        self.preview_page = 0;
        self.selection_changed();
        self.title_field
            .update(cx, |field, cx| field.set_value(sheet.name.clone(), cx));
        self.sync_paper_fields(cx);
        self.dirty = true;
        self.set_status(format!("Loaded sheet / {}", sheet.name));
        self.log(format!("Sheet loaded / {}", sheet.name));
        self.refresh_and_notify(cx);
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

    pub(crate) fn new_project(&mut self, cx: &mut Context<Self>) {
        if self.exporting.is_some() {
            self.set_status("Finish the export first");
            cx.notify();
            return;
        }

        self.generation = self.generation.wrapping_add(1);
        self.project = Project::new("Untitled");
        self.project_path = None;
        self.dirty = false;
        self.selected = Arc::new(HashSet::new());
        self.filter = Filter::All;
        self.search.clear();
        self.thumbs.clear();
        self.thumbs_queued.clear();
        self.thumb_order.clear();
        self.thumb_queue.clear();
        self.thumbs_in_flight = 0;
        self.rotated_thumbs.clear();
        self.grid_scroll = UniformListScrollHandle::default();
        self.import_queue.clear();
        self.importing_paths.clear();
        self.imports_in_flight = 0;
        self.import_state = None;
        self.drive_picker = None;
        self.drive_notice = None;
        self.drive_download = None;
        self.rename_pending = None;
        self.rename_options = RenameOptions::default();
        self.preview_page = 0;
        self.preview_modal_open = false;
        self.paper_menu_open = false;
        self.about_open = false;
        self.history.clear();

        self.search_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.title_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.tag_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.prefix_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.suffix_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.sync_paper_fields(cx);

        self.project_changed();
        self.selection_changed();
        self.filter_changed();
        self.refresh_preview();
        self.set_status("New project / add photos to begin");
        self.log("New project");
        memory::release_free_pages();
        cx.notify();
    }

    pub(crate) fn open_project_prompt(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Open .slate project".into()),
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
                self.drive_picker = None;
                self.drive_notice = None;
                self.thumbs.clear();
                self.thumbs_queued.clear();
                self.thumb_order.clear();
                self.rotated_thumbs.clear();
                self.thumb_queue.clear();
                self.thumbs_in_flight = 0;
                self.grid_scroll = UniformListScrollHandle::default();
                self.selected = Arc::new(project.photos.iter().map(|photo| photo.id()).collect());
                self.filter = Filter::All;
                self.search.clear();
                self.search_field
                    .update(cx, |field, cx| field.set_value("", cx));
                self.title_field.update(cx, |field, cx| {
                    field.set_value(project.sheet.title.clone(), cx)
                });
                self.project = project;
                self.sync_paper_fields(cx);
                self.project_path = Some(path.clone());
                self.dirty = false;
                self.history.clear();
                self.preview_page = 0;
                self.preview_modal_open = false;
                self.paper_menu_open = false;
                self.rename_pending = None;
                self.project_changed();
                self.selection_changed();
                self.filter_changed();
                self.remember_current_project();
                self.sync_native_menus(cx);
                self.log("File loaded");
                self.set_status(format!("Opened {} photos", self.project.photos.len()));
                self.refresh_preview();
                memory::release_free_pages();
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

    pub(crate) fn save_project_as(&mut self, cx: &mut Context<Self>) {
        let directory = self.default_directory();
        let suggested = self.suggested_file_name("slate");
        cx.spawn(async move |this, cx| {
            let Ok(receiver) =
                cx.update(|app| app.prompt_for_new_path(&directory, Some(&suggested)))
            else {
                return;
            };
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let path = ensure_extension(path, "slate");
            let _ = this.update(cx, |this, cx| this.write_project(path, cx));
        })
        .detach();
    }

    fn write_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.project.save(&path) {
            Ok(()) => {
                self.project_path = Some(path.clone());
                self.dirty = false;
                self.history.mark_saved();
                self.remember_current_project();
                self.sync_native_menus(cx);
                self.log("Write complete");
                self.set_status(format!("Project written / {}", path.display()));
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

    /// Import files that are already known to be images. Drive downloads use
    /// this because a Drive image may have no local extension.
    pub(crate) fn begin_import_files(&mut self, mut paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        paths.sort();
        paths.dedup();
        self.last_import_render = Instant::now();
        self.enqueue_import(paths, cx);
    }

    pub(crate) fn begin_import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let generation = self.generation;
        self.last_import_render = Instant::now();
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

        self.checkpoint();
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
                    .background_spawn(async move { import::probe(&task_path, THUMB_EDGE) })
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
                        if this.last_import_render.elapsed() >= Duration::from_millis(100) {
                            this.last_import_render = Instant::now();
                            cx.notify();
                        }
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
                self.insert_thumb(id, ThumbState::Ready(render_image(probe.preview)));
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
                self.insert_thumb(id, ThumbState::Failed(error.to_string()));
                id
            }
        };

        if let Some(state) = &mut self.import_state {
            state.done += 1;
        }
        self.importing_paths.remove(&path);
        Arc::make_mut(&mut self.selected).insert(id);
        self.project_changed();
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
        memory::release_free_pages();
        self.refresh_and_notify(cx);
    }

    // ----- thumbnails ----------------------------------------------------

    fn insert_thumb(&mut self, id: PhotoId, state: ThumbState) {
        if !self.thumbs.contains_key(&id)
            && self.thumbs.len() >= THUMB_CACHE
            && let Some(oldest) = self.thumb_order.pop_front()
        {
            self.thumbs.remove(&oldest);
            self.rotated_thumbs
                .retain(|(photo_id, _), _| *photo_id != oldest);
            self.thumbs_queued.remove(&oldest);
        }
        self.thumb_order.retain(|existing| *existing != id);
        self.thumb_order.push_back(id);
        self.thumbs.insert(id, state);
    }

    pub(crate) fn schedule_visible_thumbs(
        &mut self,
        indices: impl IntoIterator<Item = usize>,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<PhotoId> = indices
            .into_iter()
            .filter_map(|index| self.project.photos.get(index).map(|photo| photo.id()))
            .collect();
        self.schedule_thumb_ids(ids, cx);
    }

    pub(crate) fn schedule_thumb_ids(
        &mut self,
        ids: impl IntoIterator<Item = PhotoId>,
        cx: &mut Context<Self>,
    ) {
        for id in ids {
            if self.thumbs.contains_key(&id) || self.thumbs_queued.contains(&id) {
                continue;
            }
            let Some(path) = self.project.photo(id).map(|photo| photo.path.clone()) else {
                continue;
            };
            self.thumbs_queued.insert(id);
            self.thumb_queue.push_front((id, path));
        }
        while self.thumb_queue.len() > THUMB_QUEUE_LIMIT {
            if let Some((stale, _)) = self.thumb_queue.pop_back() {
                self.thumbs_queued.remove(&stale);
            }
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
                    .background_spawn(async move { import::preview(&path, THUMB_EDGE) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.thumbs_in_flight = this.thumbs_in_flight.saturating_sub(1);
                    this.thumbs_queued.remove(&id);
                    match result {
                        Ok(preview) => {
                            this.insert_thumb(id, ThumbState::Ready(render_image(preview)));
                        }
                        Err(error) => {
                            this.insert_thumb(id, ThumbState::Failed(error.to_string()));
                        }
                    }
                    this.pump_thumbs(cx);
                    if this.thumb_queue.is_empty() && this.thumbs_in_flight == 0 {
                        memory::release_free_pages();
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(crate) fn thumb_image(
        &mut self,
        id: PhotoId,
        turns: QuarterTurns,
    ) -> Option<Arc<RenderImage>> {
        if turns == QuarterTurns::NONE {
            return match self.thumbs.get(&id) {
                Some(ThumbState::Ready(image)) => Some(image.clone()),
                _ => None,
            };
        }
        if let Some(image) = self.rotated_thumbs.get(&(id, turns)) {
            return Some(image.clone());
        }
        let base = match self.thumbs.get(&id) {
            Some(ThumbState::Ready(image)) => image.clone(),
            _ => return None,
        };
        let rotated = rotate_render_image(&base, turns)?;
        if self.rotated_thumbs.len() >= 8 {
            self.rotated_thumbs.clear();
        }
        self.rotated_thumbs.insert((id, turns), rotated.clone());
        Some(rotated)
    }

    pub(crate) fn rotate_photo(&mut self, id: PhotoId, cx: &mut Context<Self>) {
        let Some(name) = self.project.photo(id).map(|photo| photo.file_name()) else {
            return;
        };
        self.checkpoint();
        let Some(photo) = self.project.photo_mut(id) else {
            return;
        };
        photo.rotate_clockwise();
        let degrees = photo.rotation().degrees();
        self.dirty = true;
        self.log(format!("Rotate / {name} / {degrees}°"));
        self.refresh_and_notify(cx);
    }

    pub(crate) fn thumb_failed(&self, id: PhotoId) -> Option<&str> {
        match self.thumbs.get(&id) {
            Some(ThumbState::Failed(reason)) => Some(reason.as_str()),
            _ => None,
        }
    }

    pub(crate) fn preview_image(
        &mut self,
        id: PhotoId,
        user_turns: QuarterTurns,
        rotated: bool,
    ) -> Option<Arc<RenderImage>> {
        let turns = if rotated {
            user_turns.clockwise()
        } else {
            user_turns
        };
        self.thumb_image(id, turns)
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
            self.theme = if matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ) {
                theme::Theme::Dark
            } else {
                theme::Theme::Light
            };
            self.appearance_initialized = true;
            theme::configure_window_chrome();
            theme::apply_native_appearance(self.theme);
        }
        theme::set_theme(self.theme);
        self.panels.clamp_to_window(window);

        let menu = self.render_menu(cx);
        let overlay = self.render_overlay(window, cx);

        div()
            .key_context("ContactSheet")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .on_action(cx.listener(|this, _: &SaveProject, _, cx| {
                this.close_menu(cx);
                this.save_project(cx);
            }))
            .on_action(cx.listener(|this, _: &NewProject, _, cx| {
                this.close_menu(cx);
                this.new_project(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenProject, _, cx| {
                this.close_menu(cx);
                this.open_project_prompt(cx);
            }))
            .on_action(cx.listener(|this, _: &SaveAs, _, cx| {
                this.close_menu(cx);
                this.save_project_as(cx);
            }))
            .on_action(cx.listener(|this, _: &ExportSheet, _, cx| {
                this.close_menu(cx);
                this.export_current(cx);
            }))
            .on_action(cx.listener(|this, _: &ImportFiles, _, cx| {
                this.close_menu(cx);
                this.import_prompt(false, cx);
            }))
            .on_action(cx.listener(|this, _: &ImportFolder, _, cx| {
                this.close_menu(cx);
                this.import_prompt(true, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectAllPhotos, _, cx| {
                this.close_menu(cx);
                this.select_all_visible(cx);
            }))
            .on_action(cx.listener(|this, _: &DeselectPhotos, _, cx| {
                this.close_menu(cx);
                this.deselect_all(cx);
            }))
            .on_action(cx.listener(|this, _: &Undo, window, cx| {
                this.close_menu(cx);
                if !this.editing_text(window, cx) {
                    this.undo(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Redo, window, cx| {
                this.close_menu(cx);
                if !this.editing_text(window, cx) {
                    this.redo(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SaveSheet, _, cx| {
                this.close_menu(cx);
                this.save_sheet(cx);
            }))
            .on_action(cx.listener(|this, _: &InvertSelection, _, cx| {
                this.close_menu(cx);
                this.invert_visible(cx);
            }))
            .on_action(cx.listener(|this, _: &RemoveSelected, _, cx| {
                this.close_menu(cx);
                this.remove_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &ResetPanels, _, cx| {
                this.close_menu(cx);
                this.panels.reset();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &About, _, cx| {
                this.menu_open = None;
                this.about_open = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ImportDrive, _, cx| {
                if this.drive_account.is_some() {
                    this.open_drive_picker(DriveMode::Import, cx);
                } else {
                    this.link_google_drive(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ExportDrive, _, cx| {
                if this.drive_account.is_some() {
                    this.open_drive_picker(DriveMode::Export, cx);
                } else {
                    this.link_google_drive(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &LinkDrive, _, cx| {
                this.link_google_drive(cx);
            }))
            .on_action(cx.listener(|this, _: &UnlinkDrive, _, cx| {
                this.unlink_google_drive(cx);
            }))
            .on_action(cx.listener(|this, action: &OpenRecent, _, cx| {
                this.close_menu(cx);
                this.open_project(action.path.clone(), cx);
            }))
            .on_action(cx.listener(|this, action: &SetTheme, _, cx| {
                this.set_theme(action.theme, cx);
            }))
            .on_action(cx.listener(|this, _: &HideSlate, _, cx| {
                this.close_menu(cx);
                cx.hide();
            }))
            .on_action(cx.listener(|this, _: &HideOthers, _, cx| {
                this.close_menu(cx);
                cx.hide_other_apps();
            }))
            .on_action(cx.listener(|this, _: &OpenWebsite, _, cx| {
                this.close_menu(cx);
                cx.open_url("https://tom.so/work/slate");
            }))
            .on_action(cx.listener(|this, _: &ReportIssue, _, cx| {
                this.close_menu(cx);
                cx.open_url("https://github.com/et0and/slate-desktop/issues");
            }))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                if this.preview_modal_open {
                    this.preview_modal_open = false;
                } else if this.paper_menu_open {
                    this.paper_menu_open = false;
                } else if this.drive_notice.is_some() {
                    this.drive_notice = None;
                } else if this.drive_picker.is_some() {
                    this.drive_picker = None;
                } else if this.about_open {
                    this.about_open = false;
                } else if this.menu_open.is_some() {
                    this.menu_open = None;
                } else if this.rename_pending.is_some() {
                    this.cancel_rename(cx);
                }
                cx.notify();
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
                    .child(divider("resize-left", ResizeTarget::Left, cx))
                    .child(self.render_library(window, cx))
                    .child(divider("resize-right", ResizeTarget::Right, cx))
                    .child(self.render_inspector(window, cx)),
            )
            .child(self.render_statusbar())
            .children(menu)
            .children(overlay)
    }
}

fn divider(id: &'static str, target: ResizeTarget, cx: &mut Context<ContactSheet>) -> AnyElement {
    div()
        .id(id)
        .relative()
        .w(px(DIVIDER_W))
        .h_full()
        .flex_shrink_0()
        .cursor(CursorStyle::ResizeColumn)
        .hover(|style| style.bg(theme::accent_soft()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| this.begin_resize(target, cx)),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(3.5))
                .w(px(1.0))
                .h_full()
                .bg(theme::rule()),
        )
        .into_any_element()
}

fn rotate_render_image(image: &RenderImage, turns: QuarterTurns) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let bytes = image.as_bytes(0)?;
    let mut buffer =
        RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, bytes.to_vec())?;
    for _ in 0..turns.turns() {
        buffer = image::imageops::rotate90(&buffer);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}

fn render_image(preview: PreviewImage) -> Arc<RenderImage> {
    let width = preview.width();
    let height = preview.height();
    let bgra = preview.into_bgra();
    let buffer = RgbaImage::from_raw(width, height, bgra)
        .expect("preview width and height match its pixel buffer");
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}

fn paper_mm_text(mm: f32) -> String {
    let text = format!("{mm:.1}");
    text.strip_suffix(".0").unwrap_or(&text).to_string()
}

pub(crate) fn is_untitled(name: &str) -> bool {
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
