//! Left rail: folders, search, review controls, sort and filters.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, Bounds, ClickEvent, Context, ElementId, MouseButton, SharedString, Window, canvas,
    div, fill, point, prelude::*, px, rgba, size,
};

use slate_core::model::{ColorLabel, Filter, StarRating};

use crate::app::{ContactSheet, PanelSection};
use crate::assets;
use crate::drive;
use crate::theme;
use crate::view::{SortDirection, SortKey};
use crate::widgets::{
    button, group_label, icon, icon_button, micro, mono_text, segment_button, shortcut_hint,
    shortcut_label, text_button, ui_text,
};

/// One row of the Filters list.
struct FilterRow {
    filter: Filter,
    name: String,
    count: usize,
}

impl ContactSheet {
    pub(crate) fn render_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let photo_count = self.project.photos.len();
        let tag_counts = self.tag_counts();
        let label_counts = self.label_counts();
        let scope_rating = self.scope_rating();
        let scope_label = self.scope_label();
        let scope_count = self.scope_ids().len();
        let collapsed = self.collapsed_sections.clone();
        let open = |section| !collapsed.contains(&section);

        let add_folder = div()
            .id("folders-add")
            .size(px(20.0))
            .my(px(-3.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(theme::accent_soft()))
            .debug_selector(|| "folders-add".into())
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_folder_menu(event.position(), cx);
            }))
            .child(icon(assets::PLUS, 12.0, theme::muted()))
            .into_any_element();
        let folders_block = panel_section(
            PanelSection::Folders,
            "Folders",
            String::new(),
            open(PanelSection::Folders),
            self.render_folder_tree(cx),
            Some(add_folder),
            cx,
        );

        let search_block = div()
            .relative()
            .px(px(16.0))
            .py(px(12.0))
            .child(self.search_field.clone())
            .children(self.search.is_empty().then(|| {
                div()
                    .absolute()
                    .top(px(12.0))
                    .right(px(26.0))
                    .h(px(theme::FIELD))
                    .flex()
                    .items_center()
                    .child(shortcut_hint(shortcut_label("K")))
            }));

        let rating_block = panel_section(
            PanelSection::Rating,
            "Rating",
            format!("{scope_count} in scope"),
            open(PanelSection::Rating),
            div()
                .px(px(16.0))
                .pb(px(14.0))
                .flex()
                .items_center()
                .justify_between()
                .child(div().flex().items_center().gap(px(3.0)).children(
                    (1..=StarRating::MAX).map(|value| {
                        let active = scope_rating.is_some_and(|rating| rating.get() >= value);
                        rating_button(value, active, cx)
                    }),
                ))
                .child(text_button("rating-clear", "Clear").on_click(
                    cx.listener(|this, _, _, cx| this.apply_rating(StarRating::NONE, cx)),
                ))
                .into_any_element(),
            None,
            cx,
        );

        let label_block = panel_section(
            PanelSection::Label,
            "Label",
            if scope_count == 0 {
                "—".to_string()
            } else {
                scope_label.map_or_else(|| "Mixed".to_string(), |label| label.label().to_string())
            },
            open(PanelSection::Label),
            div()
                .px(px(16.0))
                .pb(px(14.0))
                .flex()
                .items_center()
                .gap(px(5.0))
                .children(
                    ColorLabel::ALL
                        .into_iter()
                        .map(|label| label_button(label, scope_label == Some(label), cx)),
                )
                .into_any_element(),
            None,
            cx,
        );

        let tags_block = panel_section(
            PanelSection::Tags,
            "Tags",
            format!("{}", tag_counts.len()),
            open(PanelSection::Tags),
            div()
                .px(px(16.0))
                .pb(px(14.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.tag_field.clone())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(button("tag-add", "Add tag").flex_1().on_click(cx.listener(
                            |this, _, _, cx| {
                                let tag = this.tag_field.read(cx).value().to_string();
                                this.apply_tag(&tag, cx);
                            },
                        )))
                        .child(text_button("tag-remove", "Remove").on_click(cx.listener(
                            |this, _, _, cx| {
                                let tag = this.tag_field.read(cx).value().to_string();
                                this.remove_tag(&tag, cx);
                            },
                        )))
                        .child(
                            text_button("tag-clear", "Clear all")
                                .on_click(cx.listener(|this, _, _, cx| this.clear_tags(cx))),
                        ),
                )
                .into_any_element(),
            None,
            cx,
        );

        let sort_block = panel_section(
            PanelSection::Sort,
            "Sort",
            self.sort.label().to_string(),
            open(PanelSection::Sort),
            div()
                .px(px(16.0))
                .pb(px(14.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(div().grid().grid_cols(2).gap(px(4.0)).children(
                    SortKey::ALL.into_iter().map(|key| {
                        let active = self.sort == key;
                        sort_button(key, active, cx)
                    }),
                ))
                .child(
                    div()
                        .id("sort-direction")
                        .w_full()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.0))
                        .border_1()
                        .border_color(theme::rule())
                        .bg(theme::surface())
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::accent_soft()))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_sort_direction(cx)))
                        .child(
                            ui_text(theme::TEXT_LABEL, theme::muted())
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(self.sort_direction.label()),
                        )
                        .child(icon(
                            if self.sort_direction == SortDirection::Ascending {
                                assets::CHEVRON_UP
                            } else {
                                assets::CHEVRON_DOWN
                            },
                            10.0,
                            theme::faint(),
                        )),
                )
                .into_any_element(),
            None,
            cx,
        );

        let mut filters = vec![
            FilterRow {
                filter: Filter::All,
                name: "All images".to_string(),
                count: photo_count,
            },
            FilterRow {
                filter: Filter::Untagged,
                name: "Untagged".to_string(),
                count: self.untagged_count(),
            },
            FilterRow {
                filter: Filter::Unrated,
                name: "Unrated".to_string(),
                count: self.unrated_count(),
            },
            FilterRow {
                filter: Filter::Rated,
                name: "Rated".to_string(),
                count: self.rated_count(),
            },
        ];
        filters.extend(tag_counts.iter().map(|(tag, count)| FilterRow {
            filter: Filter::Tag(tag.clone()),
            name: format!("#{tag}"),
            count: *count,
        }));
        filters.extend(
            ColorLabel::ALL
                .into_iter()
                .enumerate()
                .skip(1)
                .map(|(index, label)| FilterRow {
                    filter: Filter::Label(label),
                    name: format!("{} label", label.label()),
                    count: label_counts[index],
                }),
        );
        filters.push(FilterRow {
            filter: Filter::Unlabeled,
            name: "No label".to_string(),
            count: label_counts[ColorLabel::None as usize],
        });
        let active_filter = filters
            .iter()
            .find(|row| row.filter == self.filter)
            .map(|row| row.name.clone())
            .unwrap_or_default();
        let filter_rows: Vec<AnyElement> = filters
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let active = row.filter == self.filter;
                let filter = row.filter.clone();
                filter_row(index, &row, active)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_filter(filter.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let filters_block = panel_section(
            PanelSection::Filters,
            "Filters",
            active_filter,
            open(PanelSection::Filters),
            div()
                .flex()
                .flex_col()
                .children(filter_rows)
                .into_any_element(),
            None,
            cx,
        );

        let scroll = self.sidebar_scroll.clone();
        // Paint the scroll edges after layout: GPUI clamps the offset during
        // prepaint, so reading it in `render` lags a frame behind and leaves a
        // stale edge after sections collapse.
        let scroll_edges = canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let offset = scroll.offset().y;
                let max = scroll.max_offset().height;
                if offset < px(0.0) {
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin, size(bounds.size.width, px(1.0))),
                        theme::rule(),
                    ));
                }
                if max > px(0.0) && offset > -max {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.left(), bounds.bottom() - px(1.0)),
                            size(bounds.size.width, px(1.0)),
                        ),
                        theme::rule(),
                    ));
                }
            },
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full();

        div()
            .w(px(self.panels.left()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .children((!cfg!(target_os = "macos")).then(|| {
                div()
                    .h(px(theme::WINDOW_CHROME_TOP.max(36.0)))
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .pr(px(6.0))
                    .child(
                        icon_button("sidebar-collapse", assets::PANEL_LEFT).on_click(
                            cx.listener(|this, _, window, cx| this.toggle_left_pane(window, cx)),
                        ),
                    )
            }))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        div()
                            .id("sidebar-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.sidebar_scroll)
                            .child(search_block)
                            .child(folders_block)
                            .child(rating_block)
                            .child(label_block)
                            .child(tags_block)
                            .child(sort_block)
                            .child(filters_block),
                    )
                    .child(scroll_edges),
            )
            .into_any_element()
    }

    /// Every added folder set, then the local mirror of imported Google Drive
    /// folders, which browses exactly like a local root.
    fn render_folder_tree(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mount = drive::mount_directory();
        let selected = self.selected_folder.clone();
        let expanded = self.expanded_folders.clone();
        let mut rows = Vec::new();
        for root in self.library_roots.clone() {
            let remove_root = root.clone();
            let remove = div()
                .id(ElementId::Name(SharedString::from(format!(
                    "folder-remove-{}",
                    root.display()
                ))))
                .size(px(18.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .opacity(0.0)
                .group_hover("folder-row", |style| style.opacity(1.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::rule()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.remove_library_root(&remove_root, cx);
                }))
                .child(icon(assets::X, 10.0, theme::faint()))
                .into_any_element();
            push_folder_rows(
                &root,
                0,
                selected.as_deref(),
                &expanded,
                Some(remove),
                &mut rows,
                cx,
            );
        }
        let mirrored = folder_children(&mount);
        if !mirrored.is_empty() {
            let mount_open = expanded.contains(&mount);
            let toggle = mount.clone();
            rows.push(
                div()
                    .id("folders-drive")
                    .h(px(28.0))
                    .w_full()
                    .pl(px(12.0))
                    .pr(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::accent_soft()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_folder(toggle.clone(), cx);
                    }))
                    .child(folder_chevron(&mount, true, mount_open, cx))
                    .child(icon(assets::CLOUD, 14.0, theme::faint()))
                    .child(
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .child("Google Drive"),
                    )
                    .into_any_element(),
            );
            if mount_open {
                for folder in mirrored {
                    push_folder_rows(
                        &folder,
                        1,
                        selected.as_deref(),
                        &expanded,
                        None,
                        &mut rows,
                        cx,
                    );
                }
            }
        }
        div()
            .pb(px(if rows.is_empty() { 0.0 } else { 8.0 }))
            .children(rows)
            .into_any_element()
    }

    pub(crate) fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.expanded_folders.remove(&path) {
            self.expanded_folders.insert(path);
        }
        cx.notify();
    }

    pub(crate) fn toggle_section(&mut self, section: PanelSection, cx: &mut Context<Self>) {
        if !self.collapsed_sections.remove(&section) {
            self.collapsed_sections.insert(section);
        }
        cx.notify();
    }
}

pub fn panel_section(
    section: PanelSection,
    title: &'static str,
    meta: String,
    open: bool,
    content: AnyElement,
    action: Option<AnyElement>,
    cx: &mut Context<ContactSheet>,
) -> AnyElement {
    div()
        .border_t_1()
        .border_color(theme::rule())
        .flex()
        .flex_col()
        .child(
            div()
                .id(ElementId::Name(SharedString::from(format!(
                    "section-{}",
                    section.key()
                ))))
                .w_full()
                .px(px(16.0))
                .py(px(10.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .debug_selector(move || format!("section-{}", section.key()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_section(section, cx);
                }))
                .child(group_label(title))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(micro(meta))
                        .children(action)
                        .child(icon(
                            if open {
                                assets::CHEVRON_UP
                            } else {
                                assets::CHEVRON_DOWN
                            },
                            10.0,
                            theme::faint(),
                        )),
                ),
        )
        .children(open.then_some(content))
        .into_any_element()
}

fn folder_children(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut children: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|child| {
            child.is_dir()
                && child
                    .file_name()
                    .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
        })
        .collect();
    children.sort_by_cached_key(|child| {
        child
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    children
}

fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn push_folder_rows(
    path: &Path,
    depth: usize,
    selected: Option<&Path>,
    expanded: &HashSet<PathBuf>,
    trailing: Option<AnyElement>,
    rows: &mut Vec<AnyElement>,
    cx: &mut Context<ContactSheet>,
) {
    let children = folder_children(path);
    let is_expanded = expanded.contains(path);
    let is_selected = selected == Some(path);
    let row_path = path.to_path_buf();
    let chevron = folder_chevron(path, !children.is_empty(), is_expanded, cx);
    rows.push(
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "folder-row-{}",
                path.display()
            ))))
            .group("folder-row")
            .h(px(28.0))
            .w_full()
            .pl(px((depth as f32).mul_add(14.0, 12.0)))
            .pr(px(10.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .bg(if is_selected {
                theme::accent_soft()
            } else {
                theme::panel()
            })
            .hover(|style| style.bg(theme::accent_soft()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_folder(&row_path, cx);
            }))
            .child(chevron)
            .child(icon(assets::FOLDER, 14.0, theme::faint()))
            .child(
                ui_text(
                    theme::TEXT_BODY,
                    if is_selected {
                        theme::ink()
                    } else {
                        theme::muted()
                    },
                )
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .child(folder_name(path)),
            )
            .children(trailing)
            .into_any_element(),
    );
    if is_expanded {
        for child in children {
            push_folder_rows(&child, depth + 1, selected, expanded, None, rows, cx);
        }
    }
}

/// The tree's first column: an expand toggle, or an empty cell that keeps
/// childless folders aligned.
fn folder_chevron(
    path: &Path,
    has_children: bool,
    is_expanded: bool,
    cx: &mut Context<ContactSheet>,
) -> AnyElement {
    if !has_children {
        return div().size(px(18.0)).flex_shrink_0().into_any_element();
    }
    let toggle_path = path.to_path_buf();
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "folder-toggle-{}",
            path.display()
        ))))
        .size(px(18.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_soft()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_folder(toggle_path.clone(), cx);
        }))
        .child(icon(
            if is_expanded {
                assets::CHEVRON_DOWN
            } else {
                assets::CHEVRON_RIGHT
            },
            10.0,
            theme::faint(),
        ))
        .into_any_element()
}

fn rating_button(value: u8, active: bool, cx: &mut Context<ContactSheet>) -> AnyElement {
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "rating-{value}"
        ))))
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if active {
            theme::accent()
        } else {
            theme::rule()
        })
        .bg(if active {
            theme::accent()
        } else {
            theme::surface()
        })
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_soft()))
        .debug_selector(move || format!("rating-{value}"))
        .on_click(
            cx.listener(move |this, _, _, cx| this.apply_rating(StarRating::clamped(value), cx)),
        )
        .child(icon(
            if active {
                assets::STAR_FILLED
            } else {
                assets::STAR
            },
            15.0,
            if active {
                theme::accent_text()
            } else {
                theme::muted()
            },
        ))
        .into_any_element()
}

fn label_button(label: ColorLabel, active: bool, cx: &mut Context<ContactSheet>) -> AnyElement {
    let color = theme::label_color(label);
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "label-{}",
            label.label().to_lowercase()
        ))))
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if active { theme::ink() } else { theme::rule() })
        .bg(theme::surface())
        .cursor_pointer()
        .hover(|style| style.border_color(theme::ink()))
        .debug_selector(move || format!("label-{}", label.label().to_lowercase()))
        .on_click(cx.listener(move |this, _, _, cx| this.apply_label(label, cx)))
        .child(
            div()
                .size(px(12.0))
                .border_1()
                .border_color(if label == ColorLabel::None {
                    theme::rule()
                } else {
                    color
                })
                .bg(if label == ColorLabel::None {
                    theme::surface()
                } else {
                    color
                }),
        )
        .into_any_element()
}

fn sort_button(key: SortKey, active: bool, cx: &mut Context<ContactSheet>) -> AnyElement {
    let id = format!("sort-{}", key.label().to_lowercase());
    let selector = id.clone();
    segment_button(ElementId::Name(SharedString::from(id)), key.label(), active)
        .debug_selector(move || selector)
        .on_click(cx.listener(move |this, _, _, cx| this.set_sort_key(key, cx)))
        .into_any_element()
}

fn filter_row(index: usize, row: &FilterRow, active: bool) -> gpui::Stateful<gpui::Div> {
    let marker = match row.filter {
        Filter::Label(label) => div().size(px(9.0)).bg(theme::label_color(label)),
        _ => div()
            .size(px(9.0))
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(3.0)).bg(if active {
                theme::accent()
            } else {
                rgba(0x00000000)
            })),
    };
    let base = div()
        .id(("filter-row", index))
        .h(px(theme::ROW_H))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .cursor_pointer()
        .border_b_1()
        .border_color(theme::rule())
        .bg(if active {
            theme::accent_soft()
        } else {
            rgba(0x00000000)
        })
        .active(|style| style.opacity(0.8));
    let base = if active {
        base
    } else {
        base.hover(|style| style.bg(theme::accent_soft()))
    };
    base.child(
        div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .gap(px(8.0))
            .child(marker)
            .child(
                ui_text(
                    theme::TEXT_BODY,
                    if active { theme::ink() } else { theme::muted() },
                )
                .truncate()
                .child(row.name.clone()),
            ),
    )
    .child(
        mono_text(
            theme::TEXT_META,
            if active {
                theme::muted()
            } else {
                theme::faint()
            },
        )
        .child(row.count.to_string()),
    )
}
