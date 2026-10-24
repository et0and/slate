//! Drive browser, import progress and export destination UI.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Context, ElementId, SharedString, Window, div, prelude::*, px, rgba, uniform_list,
};

use crate::app::ContactSheet;
use crate::assets;
use crate::drive::{self, DriveEntry};
use crate::theme;
use crate::widgets::{
    button, button_with_icon, checkbox, disabled_button, icon, icon_button, mono_text,
    primary_button, text_button, ui_text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriveMode {
    Import,
    Export,
}

impl DriveMode {
    fn title(self) -> &'static str {
        match self {
            Self::Import => "Import from Google Drive",
            Self::Export => "Export PDF to Google Drive",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DriveFolder {
    pub id: String,
    pub name: String,
}

pub(crate) struct DrivePicker {
    pub mode: DriveMode,
    pub folder_id: String,
    pub folder_name: String,
    pub crumbs: Vec<DriveFolder>,
    pub entries: Vec<DriveEntry>,
    pub selected: HashMap<String, DriveEntry>,
    pub loading: bool,
    pub error: Option<String>,
    pub load_id: u64,
}

impl DrivePicker {
    fn new(mode: DriveMode) -> Self {
        Self {
            mode,
            folder_id: "root".to_string(),
            folder_name: "My Drive".to_string(),
            crumbs: vec![DriveFolder {
                id: "root".to_string(),
                name: "My Drive".to_string(),
            }],
            entries: Vec::new(),
            selected: HashMap::new(),
            loading: false,
            error: None,
            load_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DriveNotice {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DriveDownload {
    pub total: usize,
}

impl ContactSheet {
    // ----- account -------------------------------------------------------

    pub(crate) fn link_google_drive(&mut self, cx: &mut Context<Self>) {
        if self.drive_linking {
            return;
        }
        let config = match drive::oauth_config() {
            Ok(config) => config,
            Err(error) => {
                self.drive_notice = Some(DriveNotice {
                    title: "Google Drive setup".to_string(),
                    lines: std::iter::once(format!("{error:#}"))
                        .chain(drive::setup_steps())
                        .collect(),
                });
                cx.notify();
                return;
            }
        };

        self.close_menu(cx);
        self.drive_linking = true;
        self.set_status("Waiting for Google Drive in the browser…");
        self.log("Google Drive link started");
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { drive::link_account(&config) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.drive_linking = false;
                match result {
                    Ok(account) => {
                        this.drive_account = Some(account);
                        this.drive_notice = None;
                        this.sync_native_menus(cx);
                        this.log("Google Drive linked");
                        this.set_status("Google Drive linked");
                    }
                    Err(error) => {
                        this.log(format!("Google Drive link failed: {error:#}"));
                        this.set_status("Google Drive link failed");
                        this.drive_notice = Some(DriveNotice {
                            title: "Google Drive".to_string(),
                            lines: vec![format!("{error:#}")],
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn unlink_google_drive(&mut self, cx: &mut Context<Self>) {
        self.close_menu(cx);
        match drive::clear_account() {
            Ok(()) => {
                self.drive_account = None;
                self.drive_picker = None;
                self.drive_notice = None;
                self.sync_native_menus(cx);
                self.log("Google Drive unlinked");
                self.set_status("Google Drive unlinked");
            }
            Err(error) => {
                self.log(format!("Google Drive unlink failed: {error:#}"));
                self.set_status(format!("Google Drive unlink failed / {error:#}"));
            }
        }
        cx.notify();
    }

    // ----- browser -------------------------------------------------------

    pub(crate) fn open_drive_picker(&mut self, mode: DriveMode, cx: &mut Context<Self>) {
        if self.drive_account.is_none() {
            self.drive_notice = Some(DriveNotice {
                title: "Google Drive".to_string(),
                lines: vec!["Link Google Drive before importing or exporting.".to_string()],
            });
            cx.notify();
            return;
        }
        if self.drive_download.is_some() {
            self.set_status("A Google Drive download is already running");
            cx.notify();
            return;
        }
        if self.exporting.is_some() {
            self.set_status("An export is already running");
            cx.notify();
            return;
        }
        self.close_menu(cx);
        self.drive_picker = Some(DrivePicker::new(mode));
        self.reload_drive_folder(cx);
    }

    pub(crate) fn close_drive_picker(&mut self, cx: &mut Context<Self>) {
        if self.drive_picker.take().is_some() {
            cx.notify();
        }
    }

    fn reload_drive_folder(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.drive_account.clone() else {
            return;
        };
        let config = match drive::oauth_config() {
            Ok(config) => config,
            Err(error) => {
                if let Some(picker) = &mut self.drive_picker {
                    picker.loading = false;
                    picker.error = Some(format!("{error:#}"));
                }
                cx.notify();
                return;
            }
        };
        let Some(picker) = &mut self.drive_picker else {
            return;
        };
        picker.loading = true;
        picker.error = None;
        picker.load_id += 1;
        let load_id = picker.load_id;
        let folder_id = picker.folder_id.clone();
        let expected_folder = folder_id.clone();
        let mode = picker.mode;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { drive::list_folder(&config, &account, &folder_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(picker) = this.drive_picker.as_mut() else {
                    return;
                };
                if picker.load_id != load_id || picker.folder_id != expected_folder {
                    return;
                }
                picker.loading = false;
                match result {
                    Ok(mut entries) => {
                        if mode == DriveMode::Export {
                            entries.retain(|entry| entry.is_folder);
                        }
                        picker.entries = entries;
                        picker.error = None;
                    }
                    Err(error) => {
                        picker.error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open_drive_folder(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.drive_picker else {
            return;
        };
        picker.folder_id = id.clone();
        picker.folder_name = name.clone();
        picker.crumbs.push(DriveFolder { id, name });
        self.reload_drive_folder(cx);
    }

    pub(crate) fn open_drive_crumb(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.drive_picker else {
            return;
        };
        let Some(crumb) = picker.crumbs.get(index).cloned() else {
            return;
        };
        picker.crumbs.truncate(index + 1);
        picker.folder_id = crumb.id;
        picker.folder_name = crumb.name;
        self.reload_drive_folder(cx);
    }

    pub(crate) fn retry_drive_folder(&mut self, cx: &mut Context<Self>) {
        self.reload_drive_folder(cx);
    }

    pub(crate) fn toggle_drive_selection(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.drive_picker else {
            return;
        };
        if picker.mode != DriveMode::Import {
            return;
        }
        if picker.selected.remove(&id).is_none()
            && let Some(entry) = picker.entries.iter().find(|entry| entry.id == id)
        {
            picker.selected.insert(id, entry.clone());
        }
        cx.notify();
    }

    // ----- import --------------------------------------------------------

    pub(crate) fn import_drive_selected(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.drive_picker else {
            return;
        };
        let entries: Vec<DriveEntry> = picker.selected.values().cloned().collect();
        let folder_name = picker.folder_name.clone();
        if entries.is_empty() {
            self.set_status("Select at least one Google Drive image");
            cx.notify();
            return;
        }
        self.begin_drive_import(entries, folder_name, cx);
    }

    pub(crate) fn import_drive_folder(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.drive_picker else {
            return;
        };
        let folder_id = picker.folder_id.clone();
        let folder_name = picker.folder_name.clone();
        let Some(account) = self.drive_account.clone() else {
            return;
        };
        let config = match drive::oauth_config() {
            Ok(config) => config,
            Err(error) => {
                self.drive_notice = Some(DriveNotice {
                    title: "Google Drive".to_string(),
                    lines: vec![format!("{error:#}")],
                });
                cx.notify();
                return;
            }
        };

        let generation = self.generation;
        self.drive_picker = None;
        self.set_status("Scanning Google Drive…");
        self.log(format!("Drive scan / {folder_name}"));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    drive::collect_folder_images(&config, &account, &folder_id)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(entries) if entries.is_empty() => {
                        this.set_status("No images found in that Google Drive folder");
                        this.log("Drive scan / 0 images");
                        cx.notify();
                    }
                    Ok(entries) => this.begin_drive_import(entries, folder_name, cx),
                    Err(error) => {
                        this.set_status(format!("Drive scan failed / {error:#}"));
                        this.log(format!("Drive scan failed: {error:#}"));
                        this.drive_notice = Some(DriveNotice {
                            title: "Google Drive".to_string(),
                            lines: vec![format!("{error:#}")],
                        });
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn begin_drive_import(
        &mut self,
        entries: Vec<DriveEntry>,
        folder_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(account) = self.drive_account.clone() else {
            return;
        };
        let config = match drive::oauth_config() {
            Ok(config) => config,
            Err(error) => {
                self.drive_notice = Some(DriveNotice {
                    title: "Google Drive".to_string(),
                    lines: vec![format!("{error:#}")],
                });
                cx.notify();
                return;
            }
        };

        let base = self
            .project_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(slate_core::store::data_directory);
        let directory = base
            .join("Drive imports")
            .join(crate::app::slug(&folder_name));
        let total = entries.len();
        let generation = self.generation;
        if crate::app::is_untitled(&self.project.name) && !folder_name.is_empty() {
            self.project.name = folder_name.clone();
            self.dirty = true;
        }
        self.drive_picker = None;
        self.drive_download = Some(DriveDownload { total });
        self.set_status(format!(
            "Downloading {total} from Google Drive / {}",
            directory.display()
        ));
        self.log(format!("Drive download / {total} files"));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    drive::download_images(&config, &account, entries, &directory)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.drive_download = None;
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(summary) => {
                        for error in summary.errors.iter().take(8) {
                            this.log(format!("Drive skip / {error}"));
                        }
                        let errors = summary.errors.len();
                        if summary.paths.is_empty() {
                            this.set_status(format!("Drive download failed / {errors} errors"));
                            cx.notify();
                            return;
                        }
                        this.log(format!(
                            "Drive download / {} files / {} errors",
                            summary.paths.len(),
                            errors
                        ));
                        this.begin_import_files(summary.paths, cx);
                    }
                    Err(error) => {
                        this.set_status(format!("Drive download failed / {error:#}"));
                        this.log(format!("Drive download failed: {error:#}"));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    // ----- export --------------------------------------------------------

    pub(crate) fn export_drive_here(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.drive_picker else {
            return;
        };
        let folder_id = picker.folder_id.clone();
        let folder_name = picker.folder_name.clone();
        let Some(account) = self.drive_account.clone() else {
            return;
        };
        let config = match drive::oauth_config() {
            Ok(config) => config,
            Err(error) => {
                self.drive_notice = Some(DriveNotice {
                    title: "Google Drive".to_string(),
                    lines: vec![format!("{error:#}")],
                });
                cx.notify();
                return;
            }
        };
        if self.exporting.is_some() {
            self.set_status("Export already in progress");
            cx.notify();
            return;
        }

        let photos = self.export_photos();
        if photos.is_empty() {
            self.set_status("No photos to export");
            cx.notify();
            return;
        }
        let title = self.project.sheet.title_or(&self.project.name).to_string();
        let project_name = self.project.name.clone();
        let settings = self.project.sheet.clone();
        let file_name = format!("{}.pdf", crate::app::slug(&title));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp_path =
            std::env::temp_dir().join(format!("slate-drive-{}-{now}.pdf", std::process::id()));

        self.exporting = Some(format!("Google Drive / {folder_name}"));
        self.drive_picker = None;
        self.set_status(format!("Exporting to Google Drive / {folder_name}"));
        self.log(format!(
            "Drive export start / {} photos / {}",
            photos.len(),
            folder_name
        ));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let result = (|| {
                        let summary = slate_core::pdf::export_pdf(
                            &project_name,
                            &settings,
                            &photos,
                            &temp_path,
                        )?;
                        let uploaded = drive::upload_pdf(
                            &config, &account, &folder_id, &temp_path, &file_name,
                        )?;
                        Ok::<_, anyhow::Error>((summary, uploaded))
                    })();
                    let _ = std::fs::remove_file(&temp_path);
                    result
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.exporting = None;
                match result {
                    Ok((summary, uploaded)) => {
                        this.log(format!(
                            "Drive export / {} / {} pages / {} KB",
                            uploaded.name,
                            summary.pages,
                            summary.bytes / 1024
                        ));
                        this.set_status(format!("Uploaded {} to Google Drive", uploaded.name));
                        this.log(format!(
                            "Drive file / {} / {} / {}",
                            uploaded.name, uploaded.id, uploaded.url
                        ));
                    }
                    Err(error) => {
                        this.log(format!("Drive export failed: {error:#}"));
                        this.set_status(format!("Drive export failed / {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ----- render --------------------------------------------------------

    pub(crate) fn render_drive_notice(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notice = self.drive_notice.clone()?;
        let close = cx.listener(|this, _, _, cx| {
            this.drive_notice = None;
            cx.notify();
        });
        let panel = div()
            .w(px(620.0))
            .max_h(px(640.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::rule())
            .shadow_2xl()
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        ui_text(theme::TEXT_BODY, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(notice.title),
                    )
                    .child(icon_button("drive-notice-close-icon", assets::X).on_click(close)),
            )
            .child(
                div()
                    .id("drive-notice-body")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px(px(20.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(notice.lines.into_iter().map(|line| {
                        ui_text(theme::TEXT_META + 0.5, theme::muted())
                            .line_height(px(18.0))
                            .child(line)
                            .into_any_element()
                    })),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        button_with_icon(
                            "drive-open-console",
                            assets::EXTERNAL_LINK,
                            "Open Google Cloud Console",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_menu(cx);
                            cx.open_url("https://console.cloud.google.com/apis/credentials");
                        })),
                    )
                    .child(button("drive-notice-close", "Close").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.drive_notice = None;
                            cx.notify();
                        },
                    ))),
            );
        Some(modal(panel))
    }

    pub(crate) fn render_drive_picker(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let picker = self.drive_picker.as_ref()?;
        let mode = picker.mode;
        let folder_name = picker.folder_name.clone();
        let crumbs = picker.crumbs.clone();
        let loading = picker.loading;
        let error = picker.error.clone();
        let entries: Vec<DriveEntry> = picker
            .entries
            .iter()
            .filter(|entry| mode == DriveMode::Import || entry.is_folder)
            .cloned()
            .collect();
        let selected_ids: HashSet<String> = picker.selected.keys().cloned().collect();
        let selected_count = picker.selected.len();

        let mut breadcrumb = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .gap(px(3.0))
            .overflow_hidden();
        for (index, crumb) in crumbs.iter().enumerate() {
            if index > 0 {
                breadcrumb =
                    breadcrumb.child(mono_text(theme::TEXT_META, theme::faint()).child("/"));
            }
            if index + 1 == crumbs.len() {
                breadcrumb = breadcrumb.child(
                    ui_text(theme::TEXT_META, theme::ink())
                        .truncate()
                        .child(crumb.name.clone()),
                );
            } else {
                let crumb_index = index;
                breadcrumb = breadcrumb.child(
                    text_button(
                        ElementId::Name(SharedString::from(format!("drive-crumb-{index}"))),
                        crumb.name.clone(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_drive_crumb(crumb_index, cx);
                    })),
                );
            }
        }

        let list_height = (entries.len() as f32 * 46.0).clamp(1.0, 380.0);
        let list: AnyElement = if loading {
            centered_message("Loading Google Drive…")
        } else if let Some(error) = error {
            div()
                .h(px(180.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .px(px(24.0))
                .child(
                    ui_text(theme::TEXT_BODY, theme::muted())
                        .text_align(gpui::TextAlign::Center)
                        .child(error),
                )
                .child(
                    button("drive-retry", "Retry")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_drive_folder(cx))),
                )
                .into_any_element()
        } else if entries.is_empty() {
            centered_message(if mode == DriveMode::Import {
                "No images or folders here"
            } else {
                "No folders here"
            })
        } else {
            let rows = entries.clone();
            uniform_list(
                "drive-list",
                entries.len(),
                cx.processor(
                    move |_this, range: std::ops::Range<usize>, _: &mut Window, cx| {
                        range
                            .map(|index| {
                                let entry = rows[index].clone();
                                let is_selected = selected_ids.contains(&entry.id);
                                let is_folder = entry.is_folder;
                                let row_id = entry.id.clone();
                                let row_name = entry.name.clone();
                                let details = if is_folder {
                                    "Folder".to_string()
                                } else {
                                    format_size(entry.size)
                                };
                                div()
                                    .id(ElementId::Name(SharedString::from(format!(
                                        "drive-row-{}",
                                        entry.id
                                    ))))
                                    .h(px(46.0))
                                    .px(px(16.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .cursor_pointer()
                                    .bg(if is_selected {
                                        theme::accent_soft()
                                    } else {
                                        theme::surface()
                                    })
                                    .hover(|style| style.bg(theme::accent_soft()))
                                    .active(|style| style.opacity(0.8))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if is_folder {
                                            this.open_drive_folder(
                                                row_id.clone(),
                                                row_name.clone(),
                                                cx,
                                            );
                                        } else {
                                            this.toggle_drive_selection(row_id.clone(), cx);
                                        }
                                    }))
                                    .child(icon(
                                        if is_folder {
                                            assets::FOLDER
                                        } else {
                                            assets::IMAGE
                                        },
                                        14.0,
                                        theme::muted(),
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .gap(px(2.0))
                                            .child(
                                                ui_text(theme::TEXT_BODY, theme::ink())
                                                    .truncate()
                                                    .child(entry.name.clone()),
                                            )
                                            .child(
                                                mono_text(theme::TEXT_META, theme::faint())
                                                    .child(details),
                                            ),
                                    )
                                    .children((!is_folder && mode == DriveMode::Import).then(
                                        || {
                                            checkbox(
                                                ElementId::Name(SharedString::from(format!(
                                                    "drive-check-{}",
                                                    entry.id
                                                ))),
                                                is_selected,
                                            )
                                        },
                                    ))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    },
                ),
            )
            .h(px(list_height))
            .into_any_element()
        };

        let import_actions = || {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    button_with_icon("drive-import-folder", assets::FOLDER_PLUS, "Import folder")
                        .on_click(cx.listener(|this, _, _, cx| this.import_drive_folder(cx))),
                )
                .child(if selected_count > 0 {
                    primary_button("drive-import-selected", "Import selected")
                        .on_click(cx.listener(|this, _, _, cx| this.import_drive_selected(cx)))
                        .into_any_element()
                } else {
                    disabled_button("Import selected").into_any_element()
                })
        };

        let export_actions = || {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    button("drive-export-cancel", "Cancel")
                        .on_click(cx.listener(|this, _, _, cx| this.close_drive_picker(cx))),
                )
                .child(
                    primary_button("drive-export-here", "Export here")
                        .on_click(cx.listener(|this, _, _, cx| this.export_drive_here(cx))),
                )
        };

        let panel = div()
            .w(px(760.0))
            .max_h(px(650.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::rule())
            .shadow_2xl()
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                ui_text(16.0, theme::ink())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(21.0))
                                    .child(mode.title()),
                            )
                            .child(
                                mono_text(theme::TEXT_META, theme::faint())
                                    .child(folder_name.clone()),
                            ),
                    )
                    .child(
                        icon_button("drive-picker-close", assets::X)
                            .on_click(cx.listener(|this, _, _, cx| this.close_drive_picker(cx))),
                    ),
            )
            .child(
                div()
                    .px(px(16.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(breadcrumb),
            )
            .child(list)
            .child(
                div()
                    .px(px(20.0))
                    .py(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(mono_text(theme::TEXT_META, theme::muted()).child(
                        if mode == DriveMode::Import {
                            format!("{selected_count} selected")
                        } else {
                            "The PDF is uploaded to this folder".to_string()
                        },
                    ))
                    .child(if mode == DriveMode::Import {
                        import_actions().into_any_element()
                    } else {
                        export_actions().into_any_element()
                    }),
            );
        Some(modal(panel))
    }
}

fn modal(panel: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .bg(rgba(0x00000088)),
        )
        .child(panel)
        .into_any_element()
}

fn centered_message(message: impl Into<SharedString>) -> AnyElement {
    div()
        .h(px(180.0))
        .flex()
        .items_center()
        .justify_center()
        .child(ui_text(theme::TEXT_BODY, theme::muted()).child(message.into()))
        .into_any_element()
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}
