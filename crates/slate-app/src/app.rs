//! Application state and workflows. Panels live in `library`, `sidebar`,
//! `inspector` and `overlay`; this module owns the data and the async bridges to
//! native dialogs and the background executor.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::Instrument as _;

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, CursorStyle, ExternalPaths, FocusHandle,
    Focusable, MouseButton, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, Point, Render,
    RenderImage, ScrollHandle, TitlebarOptions, UniformListScrollHandle, Window, WindowBounds,
    WindowHandle, WindowOptions, actions, div, prelude::*, px, size,
};
use image::{Frame, RgbaImage};
use slate_core::PreviewImage;
use slate_core::batch::{self, RenameOptions, RenamePlan};
use slate_core::convert::{self, ConvertOptions, OutputFormat};
use slate_core::import::{self, Probe};
use slate_core::model::{
    ColorLabel, Filter, Paper, PhotoId, Project, QuarterTurns, SheetId, SheetSettings, StarRating,
};
use slate_core::pdf::{self, ExportSummary};
use slate_core::store::{LocalStore, RecentCollection};

use crate::about_window::AboutWindow;
use crate::drive::{self, DriveEntry};
use crate::drive_ui::{DriveDownload, DriveMode, DriveNotice, DrivePicker};
use crate::history::{History, PhotoEdit, PhotoFieldChange, Snapshot, Step};
use crate::memory;
use crate::panels::{DIVIDER_W, PanelLayout, ResizeTarget};
use crate::photo_preview::PhotoPreviewWindow;
use crate::preview_window::PreviewWindow;
use crate::selection::Selection;
use crate::text_field::TextField;
use crate::theme;
use crate::view::{SortDirection, SortKey, ViewState};

actions!(
    contact_sheet_actions,
    [
        Quit,
        NewProject,
        OpenPreview,
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
        CreateContactSheet,
        ConvertSelected,
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

#[derive(Clone, PartialEq, Eq, gpui::Action)]
#[action(no_json, namespace = contact_sheet)]
pub struct OpenRecent {
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Eq, gpui::Action)]
#[action(no_json, namespace = contact_sheet)]
pub struct SetTheme {
    pub theme: theme::Theme,
}

const THUMB_CACHE: usize = 32;
const THUMB_EDGE: u32 = 224;
const THUMB_QUEUE_LIMIT: usize = 512;
const PHOTO_PREVIEW_EDGE: u32 = 3200;
const INSPECTOR_EDGE: u32 = 1200;

#[derive(Debug, Clone)]
pub enum ThumbState {
    Ready(Arc<RenderImage>),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Marquee {
    pub start: Point<Pixels>,
    pub current: Point<Pixels>,
    pub target: bool,
    pub moved: bool,
}

#[derive(Debug, Clone)]
pub struct PhotoContextMenu {
    pub photo_id: PhotoId,
    pub position: Point<Pixels>,
}

#[derive(Debug, Clone)]
pub enum PhotoMetaState {
    Loading,
    Ready(import::PhotoMetadata),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InspectorMode {
    #[default]
    Photo,
    Sheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidebarSection {
    Folders,
    Rating,
    Label,
    Sort,
    Tags,
    Rename,
}

impl SidebarSection {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Folders => "folders",
            Self::Rating => "rating",
            Self::Label => "label",
            Self::Sort => "sort",
            Self::Tags => "tags",
            Self::Rename => "rename",
        }
    }
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

    pub(crate) selected: Arc<Selection>,
    pub(crate) filter: Filter,
    pub(crate) search: String,
    pub(crate) sort: SortKey,
    pub(crate) sort_direction: SortDirection,
    pub(crate) view: ViewState,

    pub(crate) thumbs: HashMap<PhotoId, ThumbState>,
    pub(crate) thumb_queue: VecDeque<(PhotoId, PathBuf)>,
    pub(crate) thumbs_in_flight: usize,
    pub(crate) thumbs_queued: HashSet<PhotoId>,
    pub(crate) thumb_order: VecDeque<PhotoId>,
    pub(crate) rotated_thumbs: HashMap<(PhotoId, QuarterTurns), Arc<RenderImage>>,
    pub(crate) grid_scroll: UniformListScrollHandle,
    pub(crate) rename_scroll: UniformListScrollHandle,
    pub(crate) sidebar_scroll: ScrollHandle,

    pub(crate) import_queue: VecDeque<PathBuf>,
    pub(crate) importing_paths: HashSet<PathBuf>,
    pub(crate) imports_in_flight: usize,
    pub(crate) import_selects: bool,
    pub(crate) import_state: Option<ImportState>,
    pub(crate) last_import_render: Instant,

    pub(crate) preview_page: usize,
    pub(crate) preview_window: Option<WindowHandle<PreviewWindow>>,
    pub(crate) photo_preview_window: Option<WindowHandle<PhotoPreviewWindow>>,
    pub(crate) photo_preview_target: Option<PhotoId>,
    pub(crate) photo_preview_image: Option<Arc<RenderImage>>,
    pub(crate) photo_preview_error: Option<String>,
    pub(crate) photo_preview_load_id: u64,
    pub(crate) inspector_image: Option<(PhotoId, Arc<RenderImage>)>,
    pub(crate) inspector_image_error: Option<(PhotoId, String)>,
    pub(crate) inspector_load_id: u64,
    pub(crate) about_window: Option<WindowHandle<AboutWindow>>,
    pub(crate) inspector_mode: InspectorMode,
    pub(crate) inspected_photo: Option<PhotoId>,
    pub(crate) convert_dialog_open: bool,
    pub(crate) convert_format: OutputFormat,
    pub(crate) converting: Option<String>,
    pub(crate) marquee: Option<Marquee>,
    pub(crate) paper_menu_open: bool,
    pub(crate) rename_pending: Option<Arc<RenamePlan>>,
    pub(crate) rename_applying: bool,
    pub(crate) rename_photo_target: Option<PhotoId>,
    pub(crate) rename_photo_error: Option<String>,
    pub(crate) photo_context: Option<PhotoContextMenu>,
    pub(crate) photo_meta: HashMap<PhotoId, PhotoMetaState>,
    pub(crate) photo_date_error: Option<String>,
    pub(crate) rename_options: RenameOptions,
    pub(crate) exporting: Option<String>,

    pub(crate) status: String,
    pub(crate) log: VecDeque<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) focused_once: bool,
    pub(crate) theme: theme::Theme,
    pub(crate) appearance_initialized: bool,
    pub(crate) sidebar_collapsed: HashSet<SidebarSection>,
    pub(crate) panels: PanelLayout,
    pub(crate) menu_open: Option<crate::menubar::MenuId>,
    pub(crate) drive_account: Option<drive::DriveAccount>,
    pub(crate) drive_picker: Option<DrivePicker>,
    pub(crate) drive_notice: Option<DriveNotice>,
    pub(crate) drive_thumbs: HashMap<String, Arc<RenderImage>>,
    pub(crate) drive_thumb_queue: VecDeque<DriveEntry>,
    pub(crate) drive_thumb_queued: HashSet<String>,
    pub(crate) drive_thumbs_in_flight: usize,
    pub(crate) drive_thumb_load_id: u64,
    pub(crate) rotated_drive_thumbs: HashMap<(String, i16), Arc<RenderImage>>,
    pub(crate) drive_linking: bool,
    pub(crate) drive_download: Option<DriveDownload>,
    pub(crate) history: History,
    pub(crate) store: LocalStore,
    pub(crate) recent_collections: Vec<RecentCollection>,
    pub(crate) library_root: Option<PathBuf>,
    pub(crate) selected_folder: Option<PathBuf>,
    pub(crate) expanded_folders: HashSet<PathBuf>,

    pub(crate) search_field: gpui::Entity<TextField>,
    pub(crate) title_field: gpui::Entity<TextField>,
    pub(crate) rename_photo_field: gpui::Entity<TextField>,
    pub(crate) photo_date_field: gpui::Entity<TextField>,
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
            TextField::new(cx, "Search").on_change(move |value, app| {
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

        let rename_photo_field = cx.new(|cx| TextField::new(cx, "File name"));
        let photo_date_field = cx.new(|cx| TextField::new(cx, "YYYY-MM-DD HH:MM:SS"));

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
                    let _ = submit_weak.update(app, Self::preview_rename);
                })
        });

        let tag_field = cx.new(|cx| {
            let weak = weak.clone();
            TextField::new(cx, "Tag, e.g. street").on_submit(move |value, app| {
                let value = value.to_string();
                let _ = weak.update(app, |this, cx| this.apply_tag_value(&value, cx, true));
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
        let recent_collections = store.recent_collections(5).unwrap_or_default();
        let theme = store
            .setting("theme")
            .ok()
            .flatten()
            .and_then(|key| theme::Theme::from_key(&key))
            .unwrap_or(theme::Theme::Light);

        let mut this = Self {
            project: Project::new("Untitled"),
            project_path: None,
            dirty: false,
            generation: 1,
            selected: Arc::new(Selection::new()),
            filter: Filter::All,
            search: String::new(),
            sort: SortKey::Name,
            sort_direction: SortDirection::Ascending,
            view: ViewState::new(),
            thumbs: HashMap::new(),
            thumb_queue: VecDeque::new(),
            thumbs_in_flight: 0,
            thumbs_queued: HashSet::new(),
            thumb_order: VecDeque::new(),
            rotated_thumbs: HashMap::new(),
            grid_scroll: UniformListScrollHandle::default(),
            rename_scroll: UniformListScrollHandle::default(),
            sidebar_scroll: ScrollHandle::default(),
            import_queue: VecDeque::new(),
            importing_paths: HashSet::new(),
            imports_in_flight: 0,
            import_selects: false,
            import_state: None,
            last_import_render: Instant::now(),
            preview_page: 0,
            preview_window: None,
            photo_preview_window: None,
            photo_preview_target: None,
            photo_preview_image: None,
            photo_preview_error: None,
            photo_preview_load_id: 0,
            inspector_image: None,
            inspector_image_error: None,
            inspector_load_id: 0,
            about_window: None,
            inspector_mode: InspectorMode::Photo,
            inspected_photo: None,
            convert_dialog_open: false,
            convert_format: OutputFormat::Jpeg,
            converting: None,
            marquee: None,
            paper_menu_open: false,
            rename_pending: None,
            rename_applying: false,
            rename_photo_target: None,
            rename_photo_error: None,
            photo_context: None,
            photo_meta: HashMap::new(),
            photo_date_error: None,
            rename_options: RenameOptions::default(),
            exporting: None,
            status: "Ready".to_string(),
            log,
            focus: cx.focus_handle(),
            focused_once: false,
            theme,
            appearance_initialized: false,
            sidebar_collapsed: HashSet::new(),
            panels: PanelLayout::default(),
            menu_open: None,
            drive_account: drive::load_account(),
            drive_picker: None,
            drive_notice: None,
            drive_thumbs: HashMap::new(),
            drive_thumb_queue: VecDeque::new(),
            drive_thumb_queued: HashSet::new(),
            drive_thumbs_in_flight: 0,
            drive_thumb_load_id: 0,
            rotated_drive_thumbs: HashMap::new(),
            drive_linking: false,
            drive_download: None,
            history: History::default(),
            store,
            recent_collections,
            library_root: None,
            selected_folder: None,
            expanded_folders: HashSet::new(),
            search_field,
            title_field,
            rename_photo_field,
            photo_date_field,
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
        view.ensure(
            &self.project,
            &self.filter,
            &self.search,
            &self.selected,
            self.sort,
            self.sort_direction,
        );
    }

    pub(crate) const fn project_changed(&mut self) {
        self.view.invalidate_project();
    }

    pub(crate) const fn selection_changed(&mut self) {
        self.view.invalidate_selection();
    }

    pub(crate) const fn filter_changed(&mut self) {
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

    pub(crate) fn rated_count(&mut self) -> usize {
        self.ensure_view();
        self.view.rated()
    }

    pub(crate) fn unrated_count(&mut self) -> usize {
        self.ensure_view();
        self.view.unrated()
    }

    pub(crate) fn label_counts(&mut self) -> [usize; ColorLabel::ALL.len()] {
        self.ensure_view();
        self.view.label_counts()
    }

    pub(crate) fn scope_rating(&mut self) -> Option<StarRating> {
        self.ensure_view();
        self.view.scope_rating()
    }

    pub(crate) fn scope_label(&mut self) -> Option<ColorLabel> {
        self.ensure_view();
        self.view.scope_label()
    }

    /// Explicit selection wins. With no selection, every visible photo is the
    /// working set, so import -> export needs no extra step.
    pub(crate) fn scope_ids(&mut self) -> Arc<Vec<PhotoId>> {
        self.ensure_view();
        self.view.scope()
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

    fn pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.resize_move(event, window, cx);
        self.marquee_move(event, window, cx);
    }

    pub(crate) fn toggle_left_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.toggle_left();
        window.focus(&self.focus);
        cx.notify();
    }

    pub(crate) fn toggle_right_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.toggle_right();
        window.focus(&self.focus);
        cx.notify();
    }

    fn pointer_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.end_resize(event, window, cx);
        self.end_marquee(cx);
    }

    pub(crate) fn set_theme(&mut self, theme: theme::Theme, cx: &mut Context<Self>) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        theme::set_theme(theme);
        theme::apply_native_appearance(theme);
        if let Err(error) = self.store.set_setting("theme", theme.key()) {
            self.log(format!("Settings / {error}"));
        }
        self.sync_native_menus(cx);
        let fields = [
            self.search_field.clone(),
            self.title_field.clone(),
            self.rename_photo_field.clone(),
            self.photo_date_field.clone(),
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

    pub(crate) fn open_preview_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.preview_window.as_ref()
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let page = self.current_page();
        let parent = cx.entity();
        let bounds = Bounds::centered(None, size(px(980.0), px(780.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(520.0), px(420.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("Sheet preview".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        };
        // Open after the current update: the preview reads ContactSheet while
        // it renders, so the parent must not still be borrowed by this action.
        cx.spawn(async move |this, cx| {
            let handle = cx.update(|app| {
                app.open_window(options, |_, cx| {
                    cx.new(|cx| PreviewWindow::new(parent, page, cx))
                })
            });
            let Ok(handle) = handle else {
                return;
            };
            if let Ok(handle) = handle {
                let _ = this.update(cx, |this, cx| {
                    this.preview_window = Some(handle);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn open_about_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.about_window.as_ref()
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let bounds = Bounds::centered(None, size(px(480.0), px(430.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(420.0), px(360.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("About Slate".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        };
        let handle = cx.open_window(options, |_, cx| cx.new(AboutWindow::new));
        if let Ok(handle) = handle {
            self.about_window = Some(handle);
        }
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

    fn push_photo_edit(
        &mut self,
        fields: Vec<PhotoFieldChange>,
        before_selected: Arc<Selection>,
        before_dirty: bool,
    ) {
        if fields.is_empty() {
            return;
        }
        self.history.push_photo_edit(PhotoEdit::new(
            fields,
            before_selected,
            Arc::clone(&self.selected),
            before_dirty,
            self.dirty,
        ));
    }

    fn remember_current_collection(&mut self) {
        let folder = self.selected_folder.clone().or_else(|| {
            self.project_path
                .as_ref()
                .and_then(|path| path.parent())
                .map(Path::to_path_buf)
        });
        let Some(folder) = folder else {
            return;
        };
        match self.store.remember_collection(&folder, &self.project) {
            Ok(()) => {
                self.recent_collections = self.store.recent_collections(5).unwrap_or_default();
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
        self.clear_inspector_image();
        self.grid_scroll = UniformListScrollHandle::default();
        self.project = snapshot.project;
        self.selected = snapshot.selected;
        self.dirty = snapshot.dirty;
        if photos_changed {
            self.project_changed();
        }
        self.selection_changed();
        self.preview_page = 0;
        if self
            .inspected_photo
            .is_some_and(|id| self.project.photo(id).is_none())
        {
            self.inspected_photo = None;
        }
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
            &self.rename_photo_field,
            &self.photo_date_field,
            &self.custom_width_field,
            &self.custom_height_field,
            &self.prefix_field,
            &self.suffix_field,
            &self.tag_field,
        ]
        .iter()
        .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
    }

    fn apply_history_step(&mut self, step: Step, forward: bool, cx: &mut Context<Self>) {
        match step {
            Step::Snapshot(snapshot) => self.restore(snapshot, cx),
            Step::PhotoEdit(edit) => self.apply_photo_edit(&edit, forward, cx),
        }
    }

    fn apply_photo_edit(&mut self, edit: &PhotoEdit, forward: bool, cx: &mut Context<Self>) {
        let rotation_changed = edit
            .fields()
            .iter()
            .any(|field| matches!(field, PhotoFieldChange::Rotation { .. }));
        for field in edit.fields() {
            match field {
                PhotoFieldChange::Tags { id, before, after } => {
                    if let Some(photo) = self.project.photo_mut(*id) {
                        photo.set_tags(if forward {
                            after.clone()
                        } else {
                            before.clone()
                        });
                    }
                }
                PhotoFieldChange::Rotation { id, before, after } => {
                    if let Some(photo) = self.project.photo_mut(*id) {
                        photo.rotation = if forward { *after } else { *before };
                    }
                }
                PhotoFieldChange::CapturedAt { id, before, after } => {
                    if let Some(photo) = self.project.photo_mut(*id) {
                        photo.captured_at = if forward {
                            after.clone().map(String::into_boxed_str)
                        } else {
                            before.clone().map(String::into_boxed_str)
                        };
                    }
                }
                PhotoFieldChange::Rating { id, before, after } => {
                    if let Some(photo) = self.project.photo_mut(*id) {
                        photo.set_rating(if forward { *after } else { *before });
                    }
                }
                PhotoFieldChange::Label { id, before, after } => {
                    if let Some(photo) = self.project.photo_mut(*id) {
                        photo.set_label(if forward { *after } else { *before });
                    }
                }
            }
        }
        self.selected = edit.selected(forward);
        self.dirty = edit.dirty(forward);
        self.rotated_thumbs.clear();
        if rotation_changed {
            self.clear_inspector_image();
            if let Some(id) = self.inspected_photo {
                self.load_inspector_image(id, cx);
            }
        }
        self.project_changed();
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.rename_pending.is_some() {
            self.set_status("Finish or cancel the rename first");
        } else if let Some(previous) = self.history.undo(self.snapshot()) {
            self.apply_history_step(previous, false, cx);
            self.set_status("Undone");
            self.log("Undo");
        } else {
            self.set_status("Nothing to undo");
        }
        cx.notify();
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.rename_pending.is_some() {
            self.set_status("Finish or cancel the rename first");
        } else if let Some(next) = self.history.redo(self.snapshot()) {
            self.apply_history_step(next, true, cx);
            self.set_status("Redone");
            self.log("Redo");
        } else {
            self.set_status("Nothing to redo");
        }
        cx.notify();
    }

    // ----- selection -----------------------------------------------------

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
        self.selected = Arc::new(Selection::new());
        self.selection_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn invert_visible(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible_indices();
        let selected = Arc::make_mut(&mut self.selected);
        for index in visible.iter() {
            let id = self.project.photos[*index].id();
            if !selected.remove(id) {
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
        for id in &removed_ids {
            selected.remove(*id);
            self.thumbs.remove(id);
            self.thumbs_queued.remove(id);
        }
        self.thumb_order.retain(|id| !removed_ids.contains(id));
        self.rotated_thumbs
            .retain(|(photo_id, _), _| !removed_ids.contains(photo_id));
        if self
            .inspected_photo
            .is_some_and(|id| removed_ids.contains(&id))
        {
            self.inspected_photo = None;
        }
        if self
            .photo_preview_target
            .is_some_and(|id| removed_ids.contains(&id))
        {
            self.photo_preview_target = None;
            self.photo_preview_image = None;
        }
        self.project_changed();
        self.dirty = true;
        self.log(format!("Removed {removed} photos from the collection"));
        self.set_status(format!(
            "Removed {removed} from the collection / files untouched"
        ));
        self.refresh_and_notify(cx);
    }

    // ----- tags ----------------------------------------------------------

    pub(crate) fn apply_tag(&mut self, cx: &mut Context<Self>, clear_field: bool) {
        let tag = self.tag_field.read(cx).value().to_string();
        self.apply_tag_value(&tag, cx, clear_field);
    }

    #[expect(
        clippy::needless_collect,
        reason = "snapshot before mutating the project"
    )]
    pub(crate) fn apply_tag_value(&mut self, tag: &str, cx: &mut Context<Self>, clear_field: bool) {
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
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before_tags: Vec<(PhotoId, Vec<String>)> = ids
            .iter()
            .filter_map(|id| {
                self.project
                    .photo(*id)
                    .map(|photo| (*id, photo.tags.to_vec()))
            })
            .collect();
        let changed = batch::add_tag(&mut self.project, &ids, &tag);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
            let fields: Vec<PhotoFieldChange> = before_tags
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.project.photo(id)?.tags.to_vec();
                    (before != after).then_some(PhotoFieldChange::Tags { id, before, after })
                })
                .collect();
            self.push_photo_edit(fields, before_selected, before_dirty);
        }
        if clear_field {
            self.tag_field
                .update(cx, |field, cx| field.set_value("", cx));
        }
        self.set_status(format!("Tagged {changed} / {tag} / {} in scope", ids.len()));
        self.log(format!("Tag +{tag} / {changed} photo(s)"));
        cx.notify();
    }

    #[expect(
        clippy::needless_collect,
        reason = "snapshot before mutating the project"
    )]
    pub(crate) fn remove_tag(&mut self, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        let tag = self.tag_field.read(cx).value().trim().to_string();
        if tag.is_empty() {
            self.set_status("Type the tag to remove");
            cx.notify();
            return;
        }
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before_tags: Vec<(PhotoId, Vec<String>)> = ids
            .iter()
            .filter_map(|id| {
                self.project
                    .photo(*id)
                    .map(|photo| (*id, photo.tags.to_vec()))
            })
            .collect();
        let changed = batch::remove_tag(&mut self.project, &ids, &tag);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
            let fields: Vec<PhotoFieldChange> = before_tags
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.project.photo(id)?.tags.to_vec();
                    (before != after).then_some(PhotoFieldChange::Tags { id, before, after })
                })
                .collect();
            self.push_photo_edit(fields, before_selected, before_dirty);
        }
        self.tag_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.set_status(format!("Removed tag {tag} from {changed} photo(s)"));
        self.log(format!("Tag -{tag} / {changed} photo(s)"));
        cx.notify();
    }

    #[expect(
        clippy::needless_collect,
        reason = "snapshot before mutating the project"
    )]
    pub(crate) fn clear_tags(&mut self, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before_tags: Vec<(PhotoId, Vec<String>)> = ids
            .iter()
            .filter_map(|id| {
                self.project
                    .photo(*id)
                    .map(|photo| (*id, photo.tags.to_vec()))
            })
            .collect();
        let changed = batch::clear_tags(&mut self.project, &ids);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
            let fields: Vec<PhotoFieldChange> = before_tags
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.project.photo(id)?.tags.to_vec();
                    (before != after).then_some(PhotoFieldChange::Tags { id, before, after })
                })
                .collect();
            self.push_photo_edit(fields, before_selected, before_dirty);
        }
        self.set_status(format!("Cleared tags on {changed} photo(s)"));
        self.log(format!("Tags cleared / {changed} photo(s)"));
        self.refresh_and_notify(cx);
    }

    // ----- rating, labels and sorting ------------------------------------

    #[expect(
        clippy::needless_collect,
        reason = "snapshot before mutating the project"
    )]
    pub(crate) fn apply_rating(&mut self, rating: StarRating, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("No photos in scope");
            cx.notify();
            return;
        }
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before: Vec<(PhotoId, StarRating)> = ids
            .iter()
            .filter_map(|id| self.project.photo(*id).map(|photo| (*id, photo.rating())))
            .collect();
        let changed = batch::set_rating(&mut self.project, &ids, rating);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
            let fields: Vec<PhotoFieldChange> = before
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.project.photo(id)?.rating();
                    (before != after).then_some(PhotoFieldChange::Rating { id, before, after })
                })
                .collect();
            self.push_photo_edit(fields, before_selected, before_dirty);
            self.log(format!("Rating {} / {changed} photo(s)", rating.get()));
        }
        self.set_status(if rating.is_rated() {
            format!("Rated {} / {changed} photo(s)", rating.get())
        } else {
            format!("Rating cleared / {changed} photo(s)")
        });
        self.refresh_and_notify(cx);
    }

    #[expect(
        clippy::needless_collect,
        reason = "snapshot before mutating the project"
    )]
    pub(crate) fn apply_label(&mut self, label: ColorLabel, cx: &mut Context<Self>) {
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("No photos in scope");
            cx.notify();
            return;
        }
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before: Vec<(PhotoId, ColorLabel)> = ids
            .iter()
            .filter_map(|id| self.project.photo(*id).map(|photo| (*id, photo.label())))
            .collect();
        let changed = batch::set_label(&mut self.project, &ids, label);
        self.project_changed();
        if changed > 0 {
            self.dirty = true;
            let fields: Vec<PhotoFieldChange> = before
                .into_iter()
                .filter_map(|(id, before)| {
                    let after = self.project.photo(id)?.label();
                    (before != after).then_some(PhotoFieldChange::Label { id, before, after })
                })
                .collect();
            self.push_photo_edit(fields, before_selected, before_dirty);
            self.log(format!("Label {} / {changed} photo(s)", label.label()));
        }
        self.set_status(format!("Label {} / {changed} photo(s)", label.label()));
        self.refresh_and_notify(cx);
    }

    pub(crate) fn set_sort_key(&mut self, key: SortKey, cx: &mut Context<Self>) {
        if self.sort == key {
            self.sort_direction = self.sort_direction.toggled();
        } else {
            self.sort = key;
            self.sort_direction = SortDirection::Ascending;
        }
        self.preview_page = 0;
        self.filter_changed();
        self.refresh_and_notify(cx);
    }

    pub(crate) fn toggle_sort_direction(&mut self, cx: &mut Context<Self>) {
        self.sort_direction = self.sort_direction.toggled();
        self.filter_changed();
        self.refresh_and_notify(cx);
    }

    // ----- conversion ----------------------------------------------------

    pub(crate) fn open_convert_dialog(&mut self, cx: &mut Context<Self>) {
        if self.converting.is_some() {
            self.set_status("Conversion already in progress");
            cx.notify();
            return;
        }
        if self.scope_ids().is_empty() {
            self.set_status("No photos to convert");
            cx.notify();
            return;
        }
        let count = self.scope_ids().len();
        self.convert_dialog_open = true;
        self.set_status(format!("Convert / {count} photo(s) in scope"));
        cx.notify();
    }

    pub(crate) fn close_convert_dialog(&mut self, cx: &mut Context<Self>) {
        if self.convert_dialog_open {
            self.convert_dialog_open = false;
            cx.notify();
        }
    }

    pub(crate) fn set_convert_format(&mut self, format: OutputFormat, cx: &mut Context<Self>) {
        self.convert_format = format;
        cx.notify();
    }

    pub(crate) fn start_conversion(&mut self, cx: &mut Context<Self>) {
        if self.converting.is_some() {
            self.set_status("Conversion already in progress");
            cx.notify();
            return;
        }
        let ids = Arc::new(self.scope_ids().as_ref().clone());
        if ids.is_empty() {
            self.set_status("No photos to convert");
            cx.notify();
            return;
        }
        let project = self.project.clone();
        let format = self.convert_format;
        self.convert_dialog_open = false;
        self.set_status("Choose an output folder…");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Choose output folder".into()),
                })
            }) else {
                return;
            };
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(output_dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.converting = Some(output_dir.display().to_string());
                this.set_status(format!(
                    "Converting {} photo(s) to {} / {}",
                    ids.len(),
                    format.label(),
                    output_dir.display()
                ));
                this.log(format!(
                    "Convert start / {} / {}",
                    ids.len(),
                    format.label()
                ));
                cx.notify();
            });
            let options = ConvertOptions {
                format,
                quality: 92,
            };
            let span = tracing::info_span!("app.convert", photos = ids.len(), format = ?format);
            let report = cx
                .background_spawn(async move {
                    convert::convert_photos(&project, &ids, &output_dir, options)
                })
                .instrument(span)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.converting = None;
                for (name, error) in report.errors.iter().take(4) {
                    this.log(format!("Convert error / {name}: {error}"));
                }
                this.set_status(format!("{} / {}", format.label(), report.summary()));
                this.log(format!("Convert / {}", report.summary()));
                if let Some(path) = report.outputs.first() {
                    this.set_status(format!(
                        "{} / {} / {}",
                        format.label(),
                        report.summary(),
                        path.parent()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default()
                    ));
                }
                memory::release_free_pages();
                cx.notify();
            });
        })
        .detach();
    }

    // ----- photo preview -------------------------------------------------

    pub(crate) fn clear_photo_preview(&mut self, cx: &mut Context<Self>) {
        self.photo_preview_target = None;
        self.photo_preview_image = None;
        self.photo_preview_error = None;
        self.photo_preview_load_id = self.photo_preview_load_id.wrapping_add(1);
        cx.notify();
    }

    fn close_photo_preview_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.photo_preview_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.clear_photo_preview(cx);
    }

    pub(crate) fn open_sheet_builder(&mut self, cx: &mut Context<Self>) {
        let count = self.scope_ids().len();
        if count == 0 {
            self.set_status("No photos to place on a sheet");
            cx.notify();
            return;
        }
        self.panels.expand_right();
        self.inspector_mode = InspectorMode::Sheet;
        self.preview_page = 0;
        self.set_status(format!("Contact sheet / {count} photo(s) in scope"));
        self.log(format!("Sheet builder / {count} photo(s)"));
        cx.notify();
    }

    pub(crate) fn open_photo_preview(
        &mut self,
        photo_id: PhotoId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project.photo(photo_id).is_none() {
            return;
        }
        self.inspect_photo(photo_id, cx);
        self.load_photo_preview(photo_id, cx);
        if let Some(handle) = self.photo_preview_window.as_ref()
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            let _ = window;
            return;
        }
        self.photo_preview_window = None;
        let parent = cx.entity();
        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Fullscreen(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Photo preview".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        };
        cx.spawn(async move |this, cx| {
            let handle = cx.update(|app| {
                app.open_window(options, |_, cx| {
                    cx.new(|cx| PhotoPreviewWindow::new(parent, cx))
                })
            });
            let Ok(handle) = handle else {
                return;
            };
            if let Ok(handle) = handle {
                let _ = this.update(cx, |this, cx| {
                    this.photo_preview_window = Some(handle);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn load_photo_preview(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        let Some(photo) = self.project.photo(photo_id) else {
            return;
        };
        let path = photo.path.to_path_buf();
        let turns = photo.rotation();
        self.photo_preview_target = Some(photo_id);
        self.photo_preview_image = None;
        self.photo_preview_error = None;
        self.photo_preview_load_id = self.photo_preview_load_id.wrapping_add(1);
        let load_id = self.photo_preview_load_id;
        let generation = self.generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { import::preview(&path, PHOTO_PREVIEW_EDGE) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation
                    || this.photo_preview_target != Some(photo_id)
                    || this.photo_preview_load_id != load_id
                {
                    return;
                }
                match result {
                    Ok(preview) => {
                        let image = render_image(preview);
                        this.photo_preview_image = Some(if turns == QuarterTurns::NONE {
                            image
                        } else {
                            rotate_render_image(&image, turns).unwrap_or(image)
                        });
                        this.photo_preview_error = None;
                    }
                    Err(error) => {
                        this.photo_preview_image = None;
                        this.photo_preview_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn step_photo_preview(&mut self, delta: isize, cx: &mut Context<Self>) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let current = self
            .photo_preview_target
            .and_then(|id| {
                visible
                    .iter()
                    .position(|index| self.project.photos[*index].id() == id)
            })
            .unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, visible.len() as isize - 1) as usize;
        let id = self.project.photos[visible[next]].id();
        self.inspect_photo(id, cx);
        self.load_photo_preview(id, cx);
    }

    // ----- photo rename --------------------------------------------------

    pub(crate) fn open_photo_context(
        &mut self,
        photo_id: PhotoId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.photo_context = Some(PhotoContextMenu { photo_id, position });
        cx.notify();
    }

    pub(crate) fn inspect_photo(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        self.photo_context = None;
        let Some(photo) = self.project.photo(photo_id) else {
            return;
        };
        self.panels.expand_right();
        self.inspected_photo = Some(photo_id);
        self.inspector_mode = InspectorMode::Photo;
        self.photo_date_error = None;
        let date = photo.captured_at.as_deref().unwrap_or_default().to_string();
        self.photo_date_field
            .update(cx, |field, cx| field.set_value(date, cx));
        self.schedule_thumb_ids([photo_id], cx);
        self.ensure_photo_meta(photo_id, cx);
        self.load_inspector_image(photo_id, cx);
        cx.notify();
    }

    pub(crate) fn clear_inspector_image(&mut self) {
        self.inspector_image = None;
        self.inspector_image_error = None;
        self.inspector_load_id = self.inspector_load_id.wrapping_add(1);
    }

    fn load_inspector_image(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        if matches!(self.inspector_image, Some((id, _)) if id == photo_id) {
            return;
        }
        let Some(photo) = self.project.photo(photo_id) else {
            return;
        };
        let path = photo.path.to_path_buf();
        let turns = photo.rotation();
        let generation = self.generation;
        self.inspector_load_id = self.inspector_load_id.wrapping_add(1);
        let load_id = self.inspector_load_id;
        self.inspector_image = None;
        self.inspector_image_error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { import::preview(&path, INSPECTOR_EDGE) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation
                    || this.inspected_photo != Some(photo_id)
                    || this.inspector_load_id != load_id
                {
                    return;
                }
                match result {
                    Ok(preview) => {
                        let image = render_image(preview);
                        let image = if turns == QuarterTurns::NONE {
                            image
                        } else {
                            rotate_render_image(&image, turns).unwrap_or(image)
                        };
                        this.inspector_image = Some((photo_id, image));
                        this.inspector_image_error = None;
                    }
                    Err(error) => {
                        this.inspector_image_error = Some((photo_id, error.to_string()));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn ensure_photo_meta(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        let Some(path) = self
            .project
            .photo(photo_id)
            .map(|photo| photo.path.to_path_buf())
        else {
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = self.photo_meta.entry(photo_id) {
            entry.insert(PhotoMetaState::Loading);
            let generation = self.generation;
            cx.spawn(async move |this, cx| {
                let metadata = cx
                    .background_spawn(async move { import::read_metadata(&path) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation || this.project.photo(photo_id).is_none() {
                        return;
                    }
                    if this.photo_meta.len() > 64
                        && let Some(oldest) = this.photo_meta.keys().next().copied()
                    {
                        this.photo_meta.remove(&oldest);
                    }
                    this.photo_meta
                        .insert(photo_id, PhotoMetaState::Ready(metadata));
                    if this.inspected_photo == Some(photo_id) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn save_photo_date(&mut self, cx: &mut Context<Self>) {
        let Some(photo_id) = self.inspected_photo else {
            return;
        };
        let value = self.photo_date_field.read(cx).value().trim().to_string();
        let normalized = if value.is_empty() {
            None
        } else if let Some(value) = import::normalize_capture_datetime(&value) {
            Some(value)
        } else {
            self.photo_date_error = Some("Use YYYY-MM-DD HH:MM:SS, or leave blank.".to_string());
            cx.notify();
            return;
        };
        let Some(photo) = self.project.photo(photo_id) else {
            return;
        };
        if photo.captured_at.as_deref() == normalized.as_deref() {
            self.photo_date_error = None;
            cx.notify();
            return;
        }
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before = photo.captured_at.as_deref().map(str::to_owned);
        if let Some(photo) = self.project.photo_mut(photo_id) {
            photo.captured_at = normalized.clone().map(String::into_boxed_str);
        }
        self.dirty = true;
        self.push_photo_edit(
            vec![PhotoFieldChange::CapturedAt {
                id: photo_id,
                before,
                after: normalized,
            }],
            before_selected,
            before_dirty,
        );
        self.project_changed();
        self.filter_changed();
        self.photo_date_error = None;
        self.set_status("Capture date updated / collection unsaved");
        self.log("Photo date edited");
        cx.notify();
    }

    pub(crate) fn use_file_date(&mut self, cx: &mut Context<Self>) {
        let Some(photo_id) = self.inspected_photo else {
            return;
        };
        let Some(path) = self.project.photo(photo_id).map(|photo| photo.path.clone()) else {
            return;
        };
        match import::capture_datetime(&path) {
            Some(date) => {
                self.photo_date_field
                    .update(cx, |field, cx| field.set_value(date, cx));
                self.photo_date_error = None;
            }
            None => self.photo_date_error = Some("No file date is available.".to_string()),
        }
        cx.notify();
    }

    pub(crate) fn copy_photo_path(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        let Some(path) = self.project.photo(photo_id).map(|photo| photo.path.clone()) else {
            return;
        };
        let text = path.to_string_lossy().into_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.photo_context = None;
        self.set_status("File path copied");
        self.log("File path copied");
        cx.notify();
    }

    pub(crate) fn reveal_photo(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        let Some(path) = self
            .project
            .photo(photo_id)
            .map(|photo| photo.path.to_path_buf())
        else {
            return;
        };
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map(|_| ());
        #[cfg(not(target_os = "macos"))]
        let result = open::that(&path);
        match result {
            Ok(()) => {
                self.photo_context = None;
                self.set_status("Revealed in Finder");
                self.log("Revealed in Finder");
            }
            Err(error) => {
                self.set_status(format!("Could not reveal file / {error}"));
                self.log(format!("Reveal failed: {error}"));
            }
        }
        cx.notify();
    }

    pub(crate) fn remove_photo(&mut self, photo_id: PhotoId, cx: &mut Context<Self>) {
        if self.project.photo(photo_id).is_none() {
            return;
        }
        self.checkpoint();
        self.project.remove_photos(&[photo_id]);
        Arc::make_mut(&mut self.selected).remove(photo_id);
        self.thumbs.remove(&photo_id);
        self.thumbs_queued.remove(&photo_id);
        self.thumb_order.retain(|id| *id != photo_id);
        self.rotated_thumbs.retain(|(id, _), _| *id != photo_id);
        self.photo_context = None;
        if self.inspected_photo == Some(photo_id) {
            self.inspected_photo = None;
        }
        if self.photo_preview_target == Some(photo_id) {
            self.photo_preview_target = None;
            self.photo_preview_image = None;
        }
        self.photo_meta.remove(&photo_id);
        self.project_changed();
        self.dirty = true;
        self.set_status("Removed photo from the collection / file untouched");
        self.log("Photo removed from collection");
        self.refresh_and_notify(cx);
    }

    pub(crate) fn begin_photo_rename(
        &mut self,
        photo_id: PhotoId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.photo_context = None;
        let Some(photo) = self.project.photo(photo_id) else {
            return;
        };
        let name = photo.file_name();
        self.rename_photo_target = Some(photo_id);
        self.rename_photo_error = None;
        self.rename_photo_field
            .update(cx, |field, cx| field.set_value(name, cx));
        let field = self.rename_photo_field.read(cx).focus_handle(cx);
        window.focus(&field);
        cx.notify();
    }

    pub(crate) fn cancel_photo_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename_photo_target.take().is_some() {
            self.rename_photo_error = None;
            cx.notify();
        }
    }

    pub(crate) fn apply_photo_rename(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.rename_photo_target else {
            return;
        };
        let value = self.rename_photo_field.read(cx).value().trim().to_string();
        let Some(photo) = self.project.photo(id) else {
            self.cancel_photo_rename(cx);
            return;
        };
        let from = photo.path.to_path_buf();
        let extension = photo.extension();
        let mut name = batch::sanitize_fragment(&value);
        if name.is_empty() {
            self.rename_photo_error = Some("Enter a file name.".to_string());
            cx.notify();
            return;
        }
        if Path::new(&name).extension().is_none() && !extension.is_empty() {
            name = format!("{name}.{extension}");
        }
        let Some(parent) = from.parent() else {
            self.rename_photo_error = Some("The photo has no parent folder.".to_string());
            cx.notify();
            return;
        };
        let to = parent.join(&name);
        if to == from {
            self.cancel_photo_rename(cx);
            return;
        }
        if to.exists() {
            self.rename_photo_error = Some("A file with this name already exists.".to_string());
            cx.notify();
            return;
        }
        if let Err(error) = std::fs::rename(&from, &to) {
            self.rename_photo_error = Some(error.to_string());
            cx.notify();
            return;
        }
        if let Some(photo) = self.project.photo_mut(id) {
            photo.set_path(to);
        }
        self.history.clear();
        self.dirty = true;
        self.rename_photo_target = None;
        self.rename_photo_error = None;
        self.set_status(format!("Renamed to {name}"));
        self.log(format!("Rename / {name}"));
        if let Some(path) = self.project_path.clone() {
            self.write_project(path, cx);
        }
        self.project_changed();
        self.refresh_and_notify(cx);
    }

    // ----- rename --------------------------------------------------------

    pub(crate) fn append_rename_token(&mut self, token: &str, cx: &mut Context<Self>) {
        self.rename_options.prefix.push_str(token);
        let prefix = self.rename_options.prefix.clone();
        self.prefix_field
            .update(cx, |field, cx| field.set_value(prefix, cx));
        cx.notify();
    }

    pub(crate) fn preview_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename_applying {
            self.set_status("Rename already in progress");
            cx.notify();
            return;
        }
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("Select photos to rename");
            cx.notify();
            return;
        }
        let project = self.project.clone();
        let options = self.rename_options.clone();
        let generation = self.generation;
        self.rename_pending = None;
        self.set_status("Planning rename…");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let plan = cx
                .background_spawn(async move { batch::plan_rename(&project, &ids, &options) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if plan.entries.is_empty() {
                    this.set_status("No changes / names already match");
                    this.log(format!("Rename: 0 changes / {} skipped", plan.skipped));
                    cx.notify();
                    return;
                }
                if plan.conflicts > 0 {
                    this.set_status(format!("{} rename conflict(s)", plan.conflicts));
                } else {
                    this.set_status(format!("Rename ready / {} files", plan.entries.len()));
                }
                this.rename_scroll = UniformListScrollHandle::default();
                this.rename_pending = Some(Arc::new(plan));
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn apply_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename_applying {
            self.set_status("Rename already in progress");
            cx.notify();
            return;
        }
        let Some(plan) = self.rename_pending.take() else {
            return;
        };
        if !plan.can_apply() {
            self.rename_pending = Some(plan);
            self.set_status("Fix rename conflicts first");
            cx.notify();
            return;
        }
        self.rename_applying = true;
        let generation = self.generation;
        self.set_status("Renaming files…");
        self.log(format!("Rename start / {} files", plan.entries.len()));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let report = cx
                .background_spawn(async move { batch::apply_rename_paths(&plan) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.rename_applying = false;
                if this.generation != generation {
                    return;
                }
                for (photo_id, path, _) in &report.renamed {
                    if let Some(photo) = this.project.photo_mut(*photo_id) {
                        photo.set_path(path.clone());
                    }
                }
                if report.renamed_count() > 0 {
                    this.project_changed();
                    this.dirty = true;
                    this.history.clear();
                    this.log("History cleared after rename");
                }
                for (_, error) in &report.errors {
                    this.log(format!("Rename error: {error}"));
                }
                if report.renamed_count() > 0 {
                    if let Some(path) = this.project_path.clone() {
                        this.write_project(path, cx);
                    } else {
                        this.log("Rename applied / collection unsaved");
                    }
                }
                this.set_status(format!(
                    "Renamed {} file(s){}",
                    report.renamed_count(),
                    if report.errors.is_empty() {
                        String::new()
                    } else {
                        format!(" / {} error(s)", report.errors.len())
                    }
                ));
                this.log(format!(
                    "Rename / {} ok / {} error",
                    report.renamed_count(),
                    report.errors.len()
                ));
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename_applying {
            self.set_status("Rename already in progress");
            cx.notify();
            return;
        }
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
        if self.project.saved_sheets.len() == before {
            self.history.discard_last();
        } else {
            self.dirty = true;
            self.log("Sheet deleted");
            self.set_status("Sheet deleted / collection unsaved");
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
            self.project.sheet.title.clone_from(&sheet.name);
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
        self.inspector_mode = InspectorMode::Sheet;
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

    pub(crate) fn collection_root(folder: &Path) -> PathBuf {
        folder
            .parent()
            .map_or_else(|| folder.to_path_buf(), Path::to_path_buf)
    }

    fn default_directory(&self) -> PathBuf {
        self.selected_folder
            .clone()
            .or_else(|| {
                self.project_path
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn suggested_file_name(&self, extension: &str) -> String {
        if extension == "slate" && self.selected_folder.is_some() {
            return "collection.slate".to_string();
        }
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
        self.selected_folder = None;
        self.dirty = false;
        self.selected = Arc::new(Selection::new());
        self.filter = Filter::All;
        self.search.clear();
        self.thumbs.clear();
        self.thumbs_queued.clear();
        self.thumb_order.clear();
        self.thumb_queue.clear();
        self.thumbs_in_flight = 0;
        self.rotated_thumbs.clear();
        self.clear_inspector_image();
        self.grid_scroll = UniformListScrollHandle::default();
        self.import_queue.clear();
        self.importing_paths.clear();
        self.imports_in_flight = 0;
        self.import_selects = false;
        self.import_state = None;
        self.drive_picker = None;
        self.drive_notice = None;
        self.drive_download = None;
        self.rename_pending = None;
        self.rename_applying = false;
        self.rename_photo_target = None;
        self.rename_photo_error = None;
        self.photo_context = None;
        self.photo_meta.clear();
        self.photo_date_error = None;
        self.rename_options = RenameOptions::default();
        self.preview_page = 0;
        self.inspector_mode = InspectorMode::Photo;
        self.inspected_photo = None;
        self.convert_dialog_open = false;
        self.convert_format = OutputFormat::Jpeg;
        self.close_photo_preview_window(cx);
        self.marquee = None;
        self.paper_menu_open = false;
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
        self.set_status("New collection / open a folder or add photos");
        self.log("New collection");
        memory::release_free_pages();
        cx.notify();
    }

    fn backfill_capture_dates(&mut self, cx: &mut Context<Self>) {
        let missing: Vec<(PhotoId, PathBuf)> = self
            .project
            .photos
            .iter()
            .filter(|photo| photo.captured_at.is_none())
            .map(|photo| (photo.id(), photo.path.to_path_buf()))
            .collect();
        if missing.is_empty() {
            return;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let dates = cx
                .background_spawn(async move {
                    missing
                        .into_iter()
                        .filter_map(|(id, path)| {
                            import::capture_datetime(&path).map(|date| (id, date))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let mut changed = false;
                for (id, date) in dates {
                    if let Some(photo) = this.project.photo_mut(id)
                        && photo.captured_at.is_none()
                    {
                        photo.captured_at = Some(date.into_boxed_str());
                        changed = true;
                    }
                }
                if changed {
                    this.project_changed();
                    this.filter_changed();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_project_prompt(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(receiver) = cx.update(|app| {
                app.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Open folder".into()),
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
            let _ = this.update(cx, |this, cx| {
                let root = Self::collection_root(&path);
                this.library_root = Some(root.clone());
                this.expanded_folders.insert(root);
                this.expanded_folders.insert(path.clone());
                this.open_folder(&path, cx);
            });
        })
        .detach();
    }

    fn persist_current_collection(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        let Some(path) = self.project_path.clone() else {
            self.set_status("Save the current collection before switching folders");
            return false;
        };
        match self.project.save(&path) {
            Ok(()) => {
                self.dirty = false;
                self.history.mark_saved();
                self.remember_current_collection();
                self.log("Collection saved before switch");
                true
            }
            Err(error) => {
                self.log(format!("Save before switch failed: {error}"));
                self.set_status(format!("Save failed / {error}"));
                false
            }
        }
    }

    pub(crate) fn open_project(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !self.persist_current_collection() {
            return;
        }
        if path.is_dir() {
            let root = Self::collection_root(path);
            self.library_root = Some(root.clone());
            self.expanded_folders.insert(root);
            self.expanded_folders.insert(path.to_path_buf());
            self.open_folder(path, cx);
            return;
        }
        match Project::load(path) {
            Ok(project) => {
                let folder = path
                    .parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                let root = Self::collection_root(&folder);
                self.library_root = Some(root.clone());
                self.expanded_folders.insert(root);
                self.expanded_folders.insert(folder.clone());
                self.apply_loaded_collection(project, path.to_path_buf(), folder, false, cx);
                self.sync_folder_photos(cx);
            }
            Err(error) => {
                self.log(format!("Open failed: {error}"));
                self.set_status(format!("Open failed / {error}"));
                cx.notify();
            }
        }
    }

    pub(crate) fn open_folder(&mut self, folder: &Path, cx: &mut Context<Self>) {
        if !self.persist_current_collection() {
            return;
        }
        if self.exporting.is_some() {
            self.set_status("Finish the export first");
            cx.notify();
            return;
        }
        if self.library_root.is_none() {
            let root = Self::collection_root(folder);
            self.library_root = Some(root.clone());
            self.expanded_folders.insert(root);
        }
        let collection_path = folder.join("collection.slate");
        let (collection, dirty) = if collection_path.exists() {
            match Project::load(&collection_path) {
                Ok(collection) => (collection, false),
                Err(error) => {
                    self.log(format!("Open failed: {error}"));
                    self.set_status(format!("Open failed / {error}"));
                    cx.notify();
                    return;
                }
            }
        } else {
            let name = folder.file_name().map_or_else(
                || "Collection".to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            (Project::new(name), true)
        };
        self.expanded_folders.insert(folder.to_path_buf());
        self.apply_loaded_collection(collection, collection_path, folder.to_path_buf(), dirty, cx);
        self.sync_folder_photos(cx);
    }

    pub(crate) fn open_recent(&mut self, path: &Path, cx: &mut Context<Self>) {
        if path.is_dir() {
            let root = Self::collection_root(path);
            self.library_root = Some(root.clone());
            self.expanded_folders.insert(root);
            self.expanded_folders.insert(path.to_path_buf());
            self.open_folder(path, cx);
        } else {
            self.open_project(path, cx);
        }
    }

    fn apply_loaded_collection(
        &mut self,
        collection: Project,
        collection_path: PathBuf,
        folder: PathBuf,
        dirty: bool,
        cx: &mut Context<Self>,
    ) {
        self.generation += 1;
        self.import_queue.clear();
        self.importing_paths.clear();
        self.imports_in_flight = 0;
        self.import_selects = false;
        self.import_state = None;
        self.drive_picker = None;
        self.drive_notice = None;
        self.thumbs.clear();
        self.thumbs_queued.clear();
        self.thumb_order.clear();
        self.rotated_thumbs.clear();
        self.thumb_queue.clear();
        self.thumbs_in_flight = 0;
        self.clear_inspector_image();
        self.grid_scroll = UniformListScrollHandle::default();
        self.selected = Arc::new(Selection::new());
        self.filter = Filter::All;
        self.search.clear();
        self.search_field
            .update(cx, |field, cx| field.set_value("", cx));
        self.title_field.update(cx, |field, cx| {
            field.set_value(collection.sheet.title.clone(), cx);
        });
        self.project = collection;
        self.sync_paper_fields(cx);
        self.backfill_capture_dates(cx);
        self.project_path = Some(collection_path);
        self.selected_folder = Some(folder);
        self.dirty = dirty;
        self.history.clear();
        self.preview_page = 0;
        self.inspector_mode = InspectorMode::Photo;
        self.inspected_photo = None;
        self.convert_dialog_open = false;
        self.convert_format = OutputFormat::Jpeg;
        self.close_photo_preview_window(cx);
        self.marquee = None;
        self.paper_menu_open = false;
        self.rename_pending = None;
        self.rename_applying = false;
        self.rename_photo_target = None;
        self.rename_photo_error = None;
        self.photo_context = None;
        self.photo_meta.clear();
        self.photo_date_error = None;
        self.project_changed();
        self.selection_changed();
        self.filter_changed();
        self.remember_current_collection();
        self.sync_native_menus(cx);
        self.log("Collection loaded");
        self.set_status(format!("Opened {} photos", self.project.photos.len()));
        self.refresh_preview();
        memory::release_free_pages();
        cx.notify();
    }

    fn sync_folder_photos(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_folder.clone() else {
            return;
        };
        let existing: HashSet<&Path> = self
            .project
            .photos
            .iter()
            .map(|photo| photo.path.as_ref())
            .collect();
        let missing: Vec<PathBuf> = import::discover_folder_images(&folder)
            .into_iter()
            .filter(|path| !existing.contains(path.as_path()))
            .collect();
        if !missing.is_empty() {
            self.begin_import_files(missing, cx);
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
        let project = self.project.clone();
        let generation = self.generation;
        let task_path = path.clone();
        let was_dirty = self.dirty;
        // Optimistically clear the dirty flag. Any edit during the background
        // write sets it again, so completion only marks history saved when the
        // snapshot still matches the live project.
        self.dirty = false;
        self.log(format!("Write start / {}", path.display()));
        self.set_status(format!("Saving / {}", path.display()));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let span = tracing::info_span!("app.save_project");
            let result = cx
                .background_spawn(async move { project.save(&task_path) })
                .instrument(span)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(()) => {
                        this.project_path = Some(path.clone());
                        this.selected_folder = path.parent().map(Path::to_path_buf);
                        if !this.dirty {
                            this.history.mark_saved();
                        }
                        this.remember_current_collection();
                        this.sync_native_menus(cx);
                        this.log("Write complete");
                        this.set_status(format!("Collection written / {}", path.display()));
                    }
                    Err(error) => {
                        this.dirty = was_dirty || this.dirty;
                        this.log(format!("Save failed: {error}"));
                        this.set_status(format!("Save failed / {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
        let existing: HashSet<&Path> = self
            .project
            .photos
            .iter()
            .map(|photo| photo.path.as_ref())
            .collect();
        let new_paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| {
                !existing.contains(path.as_path()) && !self.importing_paths.contains(path)
            })
            .collect();

        if new_paths.is_empty() {
            self.set_status("No new photos found");
            self.log("Import / 0 new files");
            cx.notify();
            return;
        }

        self.checkpoint();
        self.ensure_view();
        self.import_selects = !self.selected.is_empty();
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
            self.import_state.as_ref().map_or(0, |state| state.done),
            self.import_state.as_ref().map_or(0, |state| state.total)
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
                    .background_spawn(async move { import::probe(&task_path) })
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
        self.importing_paths.remove(&path);
        let id = match result {
            Ok(probe) => {
                let id = self
                    .project
                    .add_photo(path, probe.width, probe.height, probe.bytes);
                if let Some(photo) = self.project.photo_mut(id) {
                    photo.captured_at = probe.captured_at.map(String::into_boxed_str);
                }
                id
            }
            Err(error) => {
                let bytes = import::file_size(&path);
                let file_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let id = self.project.add_photo(path, 0, 0, bytes);
                if let Some(state) = &mut self.import_state
                    && state.errors.len() < 4
                {
                    state.errors.push(format!("{file_name}: {error}"));
                }
                self.insert_thumb(id, ThumbState::Failed(error.to_string()));
                id
            }
        };

        let photo_index = self.project.photos.len() - 1;
        let selected = if self.import_selects {
            Arc::make_mut(&mut self.selected).insert(id);
            true
        } else {
            false
        };
        let selected_empty = self.selected.is_empty();
        let filter = self.filter.clone();
        let search = self.search.clone();
        self.view.append_imported(
            photo_index,
            &self.project.photos[photo_index],
            &filter,
            &search,
            selected || selected_empty,
            selected,
        );

        if let Some(state) = &mut self.import_state {
            state.done += 1;
        }
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
        self.import_selects = false;
        // One full reconciliation at the end catches every incremental edge
        // case without paying an O(N) rebuild on every import tick.
        self.project_changed();
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
            .filter_map(|index| self.project.photos.get(index).map(slate_core::Photo::id))
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
            let Some(path) = self.project.photo(id).map(|photo| photo.path.to_path_buf()) else {
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
        let Some(name) = self.project.photo(id).map(slate_core::Photo::file_name) else {
            return;
        };
        let before_selected = Arc::clone(&self.selected);
        let before_dirty = self.dirty;
        let before = self.project.photo(id).map(slate_core::Photo::rotation);
        let Some(photo) = self.project.photo_mut(id) else {
            return;
        };
        photo.rotate_clockwise();
        let after = photo.rotation();
        let degrees = after.degrees();
        self.dirty = true;
        if let Some(before) = before {
            self.push_photo_edit(
                vec![PhotoFieldChange::Rotation { id, before, after }],
                before_selected,
                before_dirty,
            );
        }
        self.log(format!("Rotate / {name} / {degrees}°"));
        if self.inspected_photo == Some(id) {
            self.clear_inspector_image();
            self.load_inspector_image(id, cx);
        }
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
        let ids = self.scope_ids();
        if ids.is_empty() {
            self.set_status("No photos to export");
            cx.notify();
            return;
        }
        // Detach the export from ViewState so an import can keep appending
        // without cloning the shared scope vector for every new photo.
        let ids = Arc::new(ids.as_ref().clone());
        let title = self.project.sheet.title_or(&self.project.name).to_string();
        self.begin_export(
            title,
            self.project.sheet.clone(),
            self.project.clone(),
            ids,
            cx,
        );
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
        if sheet.photo_ids.is_empty() {
            self.set_status(format!("Sheet {} has no photos", sheet.name));
            cx.notify();
            return;
        }
        let ids = Arc::new(sheet.photo_ids.clone());
        self.begin_export(
            sheet.name.clone(),
            sheet.settings,
            self.project.clone(),
            ids,
            cx,
        );
    }

    fn begin_export(
        &mut self,
        title: String,
        settings: SheetSettings,
        project: Project,
        ids: Arc<Vec<PhotoId>>,
        cx: &mut Context<Self>,
    ) {
        if self.exporting.is_some() {
            self.set_status("Export already in progress");
            cx.notify();
            return;
        }
        let directory = self.default_directory();
        let suggested = format!("{}.pdf", slug(&title));
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
                this.log(format!("Export start / {} photos", ids.len()));
                cx.notify();
            });

            let export_settings = settings.clone();
            let export_output = output.clone();
            let span = tracing::info_span!("app.export", photos = ids.len());
            let result = cx
                .background_spawn(async move {
                    pdf::export_pdf_ids(
                        &project.name,
                        &export_settings,
                        &project,
                        &ids,
                        &export_output,
                    )
                })
                .instrument(span)
                .await;

            let _ = this.update(cx, |this, cx| {
                this.exporting = None;
                match result {
                    Ok(summary) => this.export_finished(&title, &summary),
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

    fn export_finished(&mut self, title: &str, summary: &ExportSummary) {
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
            if self.dirty {
                " / collection unsaved"
            } else {
                ""
            }
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
            .on_action(cx.listener(|this, _: &OpenPreview, window, cx| {
                this.close_menu(cx);
                this.open_preview_window(window, cx);
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
            .on_action(cx.listener(|this, _: &CreateContactSheet, _, cx| {
                this.close_menu(cx);
                this.open_sheet_builder(cx);
            }))
            .on_action(cx.listener(|this, _: &ConvertSelected, _, cx| {
                this.close_menu(cx);
                this.open_convert_dialog(cx);
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
                this.open_about_window(cx);
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
                this.open_recent(&action.path, cx);
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
                if this.rename_photo_target.is_some() {
                    this.cancel_photo_rename(cx);
                    return;
                }
                if this.photo_context.is_some() {
                    this.photo_context = None;
                    cx.notify();
                    return;
                }
                if this.convert_dialog_open {
                    this.close_convert_dialog(cx);
                    return;
                }
                if this.paper_menu_open {
                    this.paper_menu_open = false;
                } else if this.drive_notice.is_some() {
                    this.drive_notice = None;
                } else if this.drive_picker.is_some() {
                    this.drive_picker = None;
                } else if this.menu_open.is_some() {
                    this.menu_open = None;
                } else if this.rename_pending.is_some() {
                    this.cancel_rename(cx);
                }
                cx.notify();
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.begin_import(paths.paths().to_vec(), cx);
            }))
            .on_mouse_move(cx.listener(Self::pointer_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::pointer_up))
            .child(self.render_window_titlebar(cx))
            .child(self.render_topbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .children(
                        (!self.panels.left_collapsed()).then(|| self.render_sidebar(window, cx)),
                    )
                    .children(
                        (!self.panels.left_collapsed())
                            .then(|| divider("resize-left", ResizeTarget::Left, cx)),
                    )
                    .child(self.render_library(window, cx))
                    .children(
                        (!self.panels.right_collapsed())
                            .then(|| divider("resize-right", ResizeTarget::Right, cx)),
                    )
                    .children(if self.panels.right_collapsed() {
                        Some(self.render_collapsed_rail(ResizeTarget::Right, cx))
                    } else {
                        Some(self.render_inspector(window, cx))
                    }),
            )
            .child(self.render_statusbar())
            .children(self.render_marquee())
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

pub fn render_image(preview: PreviewImage) -> Arc<RenderImage> {
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

pub fn is_untitled(name: &str) -> bool {
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

pub fn slug(value: &str) -> String {
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

#[cfg(target_os = "macos")]
fn reveal(path: &Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .status();
}

#[cfg(not(target_os = "macos"))]
const fn reveal(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets;

    fn ui_project() -> Project {
        let mut project = Project::new("UI test");
        project.add_photo(PathBuf::from("/tmp/a.jpg"), 4000, 3000, 1_000_000);
        project.add_photo(PathBuf::from("/tmp/b.jpg"), 3000, 4000, 2_000_000);
        project
    }

    fn draw_root(view: &gpui::Entity<ContactSheet>, cx: &mut gpui::VisualTestContext) {
        let size = cx.update(|window, _| window.viewport_size());
        cx.draw(Point::default(), size, |_, _| {
            view.clone().into_any_element()
        });
    }

    fn ui_click(
        view: &gpui::Entity<ContactSheet>,
        cx: &mut gpui::VisualTestContext,
        selector: &'static str,
    ) {
        draw_root(view, cx);
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing debug bounds for {selector}"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn rating_label_sort_and_inspector_controls_work(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        view.update(cx, |view, cx| {
            view.project = ui_project();
            view.selected = Arc::new([PhotoId(1), PhotoId(2)].into_iter().collect());
            view.selection_changed();
            view.inspected_photo = Some(PhotoId(1));
            view.inspector_mode = InspectorMode::Photo;
            view.project_changed();
            view.filter_changed();
            cx.notify();
        });
        cx.run_until_parked();

        ui_click(&view, cx, "rating-3");
        view.update(cx, |view, _| {
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().rating(),
                StarRating::try_from(3).unwrap()
            );
            assert_eq!(
                view.project.photo(PhotoId(2)).unwrap().rating(),
                StarRating::try_from(3).unwrap()
            );
        });

        ui_click(&view, cx, "label-green");
        view.update(cx, |view, _| {
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().label(),
                ColorLabel::Green
            );
            assert_eq!(
                view.project.photo(PhotoId(2)).unwrap().label(),
                ColorLabel::Green
            );
        });

        ui_click(&view, cx, "sort-date");
        view.update(cx, |view, _| {
            assert_eq!(view.sort, SortKey::CapturedAt);
            assert_eq!(view.sort_direction, SortDirection::Ascending);
        });

        ui_click(&view, cx, "sidebar-section-rating");
        view.update(cx, |view, _| {
            assert!(
                view.sidebar_collapsed.contains(&SidebarSection::Rating),
                "rating section did not collapse"
            );
        });
        ui_click(&view, cx, "sidebar-section-rating");
        view.update(cx, |view, _| {
            assert!(!view.sidebar_collapsed.contains(&SidebarSection::Rating));
        });

        ui_click(&view, cx, "cell-1");
        view.update(cx, |view, _| {
            assert_eq!(view.inspected_photo, Some(PhotoId(1)));
            assert_eq!(view.inspector_mode, InspectorMode::Photo);
        });

        view.update(cx, super::ContactSheet::open_sheet_builder);
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert_eq!(view.inspector_mode, InspectorMode::Sheet);
        });
        draw_root(&view, cx);
        assert!(cx.debug_bounds("sheet-inspector").is_some());

        view.update(cx, super::ContactSheet::open_convert_dialog);
        cx.run_until_parked();
        draw_root(&view, cx);
        assert!(cx.debug_bounds("convert-dialog").is_some());
        view.update(cx, |view, _| assert!(view.convert_dialog_open));
    }

    fn ui_sample_project() -> Project {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/sample/street-walk-01.jpg")
            .canonicalize()
            .unwrap();
        let bytes = std::fs::metadata(&path).unwrap().len();
        let mut project = Project::new("Preview");
        project.add_photo(path, 4000, 2667, bytes);
        project
    }

    #[gpui::test]
    fn inspector_loads_a_fit_preview_larger_than_the_grid_thumb(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        view.update(cx, |view, cx| {
            view.project = ui_sample_project();
            view.selected = Arc::new(std::iter::once(PhotoId(1)).collect());
            view.selection_changed();
            view.inspect_photo(PhotoId(1), cx);
        });
        for _ in 0..10 {
            cx.run_until_parked();
            if view.update(cx, |view, _| view.inspector_image.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        view.update(cx, |view, _| {
            let (id, image) = view
                .inspector_image
                .as_ref()
                .expect("inspector preview should load");
            assert_eq!(*id, PhotoId(1));
            let size = image.size(0);
            assert!(size.width.0 > THUMB_EDGE as i32);
            assert!(size.height.0 > THUMB_EDGE as i32);
        });
    }

    #[gpui::test]
    fn rating_and_label_edits_undo_as_field_deltas(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        view.update(cx, |view, cx| {
            view.project = ui_project();
            view.selected = Arc::new(std::iter::once(PhotoId(1)).collect());
            view.selection_changed();
            view.apply_rating(StarRating::try_from(4).unwrap(), cx);
            view.apply_label(ColorLabel::Red, cx);
        });
        view.update(cx, |view, _| {
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().rating(),
                StarRating::try_from(4).unwrap()
            );
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().label(),
                ColorLabel::Red
            );
        });

        view.update(cx, super::ContactSheet::undo);
        view.update(cx, |view, _| {
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().rating(),
                StarRating::try_from(4).unwrap()
            );
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().label(),
                ColorLabel::None
            );
        });

        view.update(cx, super::ContactSheet::undo);
        view.update(cx, |view, _| {
            assert_eq!(
                view.project.photo(PhotoId(1)).unwrap().rating(),
                StarRating::NONE
            );
        });
    }

    #[gpui::test]
    fn eye_button_opens_a_native_preview_window(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        view.update(cx, |view, cx| {
            view.project = ui_project();
            view.selected = Arc::new(std::iter::once(PhotoId(1)).collect());
            view.selection_changed();
            view.project_changed();
            view.filter_changed();
            cx.notify();
        });
        cx.run_until_parked();
        draw_root(&view, cx);

        let windows_before = cx.windows().len();
        ui_click(&view, cx, "preview-1");
        cx.run_until_parked();
        cx.run_until_parked();

        view.update(cx, |view, _| {
            assert_eq!(view.photo_preview_target, Some(PhotoId(1)));
            assert_eq!(view.inspected_photo, Some(PhotoId(1)));
        });
        assert!(cx.windows().len() > windows_before);
    }

    #[gpui::test]
    fn actions_open_the_sheet_builder_and_converter(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| assets::install_fonts(cx));
        let (view, cx) = cx.add_window_view(|_, cx| ContactSheet::new(cx));
        view.update(cx, |view, cx| {
            view.project = ui_project();
            view.selected = Arc::new(std::iter::once(PhotoId(1)).collect());
            view.selection_changed();
            view.project_changed();
            view.filter_changed();
            cx.notify();
        });
        cx.run_until_parked();

        cx.dispatch_action(CreateContactSheet);
        view.update(cx, |view, _| {
            assert_eq!(view.inspector_mode, InspectorMode::Sheet);
        });

        cx.dispatch_action(ConvertSelected);
        view.update(cx, |view, _| assert!(view.convert_dialog_open));
    }

    #[test]
    fn slugs_are_flat_and_safe() {
        assert_eq!(slug("Street / Walk"), "street-walk");
        assert_eq!(slug("  "), "contact-sheet");
        assert_eq!(slug("A--B"), "a-b");
    }
}
