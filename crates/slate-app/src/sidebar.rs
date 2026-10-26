//! Left rail: import, search, tags, rename and activity.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, Context, ElementId, MouseButton, SharedString, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px, rgba,
};

use slate_core::model::{ColorLabel, Filter, StarRating};

use crate::app::{ContactSheet, SidebarSection};
use crate::assets;
use crate::theme;
use crate::view::SortKey;
use crate::widgets::{
    button, group_label, icon, icon_button, micro, mono_text, text_button, ui_text,
};

impl ContactSheet {
    pub(crate) fn render_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let photo_count = self.project.photos.len();
        let tag_counts = self.tag_counts();
        let tag_count_total = tag_counts.len();
        let untagged = self.untagged_count();
        let rated = self.rated_count();
        let unrated = self.unrated_count();
        let label_counts = self.label_counts();
        let scope_rating = self.scope_rating();
        let scope_label = self.scope_label();
        let scope_count = self.scope_ids().len();

        let folders_open = !self.sidebar_collapsed.contains(&SidebarSection::Folders);
        let rating_open = !self.sidebar_collapsed.contains(&SidebarSection::Rating);
        let label_open = !self.sidebar_collapsed.contains(&SidebarSection::Label);
        let sort_open = !self.sidebar_collapsed.contains(&SidebarSection::Sort);
        let tags_open = !self.sidebar_collapsed.contains(&SidebarSection::Tags);

        let folders_block = sidebar_section(
            SidebarSection::Folders,
            "Folders",
            String::new(),
            folders_open,
            self.render_folder_tree(cx),
            cx,
        );

        let search_block = div()
            .px(px(16.0))
            .py(px(12.0))
            .child(self.search_field.clone());

        let rating_block = sidebar_section(
            SidebarSection::Rating,
            "Rating",
            format!("{scope_count} in scope"),
            rating_open,
            div()
                .px(px(16.0))
                .pb(px(14.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(div().flex().items_center().gap(px(3.0)).children(
                    (1..=StarRating::MAX).map(|value| {
                        let active = scope_rating.is_some_and(|rating| rating.get() >= value);
                        rating_button(value, active, cx)
                    }),
                ))
                .child(div().flex().justify_end().child(
                    text_button("rating-clear", "Clear").on_click(
                        cx.listener(|this, _, _, cx| this.apply_rating(StarRating::NONE, cx)),
                    ),
                ))
                .into_any_element(),
            cx,
        );

        let label_block = sidebar_section(
            SidebarSection::Label,
            "Label",
            if scope_count == 0 {
                "—".to_string()
            } else {
                scope_label.map_or_else(|| "Mixed".to_string(), |label| label.label().to_string())
            },
            label_open,
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
            cx,
        );

        let sort_block = sidebar_section(
            SidebarSection::Sort,
            "Sort",
            format!("{photo_count} photos"),
            sort_open,
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
                                .child(self.sort_direction.label()),
                        )
                        .child(icon(
                            if self.sort_direction == crate::view::SortDirection::Ascending {
                                assets::CHEVRON_UP
                            } else {
                                assets::CHEVRON_DOWN
                            },
                            10.0,
                            theme::faint(),
                        )),
                )
                .into_any_element(),
            cx,
        );

        let mut filter_rows: Vec<AnyElement> = Vec::new();
        let is_all = matches!(self.filter, Filter::All);
        filter_rows.push(
            filter_row("filter-all", "All images", photo_count, is_all)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::All;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_untagged = matches!(self.filter, Filter::Untagged);
        filter_rows.push(
            filter_row("filter-untagged", "Untagged", untagged, is_untagged)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::Untagged;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_unrated = matches!(self.filter, Filter::Unrated);
        filter_rows.push(
            filter_row("filter-unrated", "Unrated", unrated, is_unrated)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::Unrated;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_rated = matches!(self.filter, Filter::Rated);
        filter_rows.push(
            filter_row("filter-rated", "Rated", rated, is_rated)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::Rated;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );

        for (tag, count) in tag_counts.iter() {
            let active = matches!(&self.filter, Filter::Tag(active) if active == tag);
            let click_tag = tag.clone();
            filter_rows.push(
                filter_row(
                    ElementId::Name(SharedString::from(format!("filter-tag-{tag}"))),
                    &format!("#{tag}"),
                    *count,
                    active,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filter = Filter::Tag(click_tag.clone());
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        for (index, label) in ColorLabel::ALL.into_iter().enumerate().skip(1) {
            let active = matches!(&self.filter, Filter::Label(active) if *active == label);
            let count = label_counts[index];
            filter_rows.push(
                label_filter_row(label, count, active)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filter = Filter::Label(label);
                        this.filter_changed();
                        this.refresh_preview();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }

        let no_label_active = matches!(self.filter, Filter::Unlabeled);
        filter_rows.push(
            filter_row(
                "filter-unlabeled",
                "No label",
                label_counts[ColorLabel::None as usize],
                no_label_active,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.filter = Filter::Unlabeled;
                this.filter_changed();
                this.refresh_preview();
                cx.notify();
            }))
            .into_any_element(),
        );

        let tags_content = div()
            .flex()
            .flex_col()
            .children(filter_rows)
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.tag_field.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                button("tag-add", "Add tag").flex_1().on_click(
                                    cx.listener(|this, _, _, cx| this.apply_tag(cx, true)),
                                ),
                            )
                            .child(
                                text_button("tag-remove", "Remove")
                                    .on_click(cx.listener(|this, _, _, cx| this.remove_tag(cx))),
                            )
                            .child(
                                text_button("tag-clear", "Clear")
                                    .on_click(cx.listener(|this, _, _, cx| this.clear_tags(cx))),
                            ),
                    ),
            )
            .into_any_element();

        let tags_block = sidebar_section(
            SidebarSection::Tags,
            "Tags",
            format!("{tag_count_total}"),
            tags_open,
            tags_content,
            cx,
        );

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
            .child({
                let offset = self.sidebar_scroll.offset().y;
                let max_offset = self.sidebar_scroll.max_offset().height;
                let show_top_fade = offset < px(0.0);
                let show_bottom_fade = max_offset > px(0.0) && offset > -max_offset;
                let top_fade = show_top_fade.then(|| {
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .h(px(22.0))
                        .bg(linear_gradient(
                            0.0,
                            linear_color_stop(theme::panel(), 1.0),
                            linear_color_stop(theme::panel(), 0.0),
                        ))
                });
                let bottom_fade = show_bottom_fade.then(|| {
                    div()
                        .absolute()
                        .bottom(px(0.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .h(px(22.0))
                        .bg(linear_gradient(
                            0.0,
                            linear_color_stop(theme::panel(), 0.0),
                            linear_color_stop(theme::panel(), 1.0),
                        ))
                });
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
                            .child(sort_block)
                            .child(tags_block),
                    )
                    .children(top_fade)
                    .children(bottom_fade)
            })
            .into_any_element()
    }

    fn render_folder_tree(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(root) = self.library_root.clone() else {
            return div()
                .px(px(16.0))
                .pb(px(12.0))
                .child(
                    button("folders-open", "Open folder…")
                        .w_full()
                        .on_click(cx.listener(|this, _, _, cx| this.open_project_prompt(cx))),
                )
                .into_any_element();
        };
        let selected = self.selected_folder.clone();
        let expanded = self.expanded_folders.clone();
        let mut rows = Vec::new();
        push_folder_rows(&root, 0, selected.as_deref(), &expanded, &mut rows, cx);
        div().py(px(6.0)).children(rows).into_any_element()
    }

    pub(crate) fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.expanded_folders.remove(&path) {
            self.expanded_folders.insert(path);
        }
        cx.notify();
    }

    pub(crate) fn select_folder(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.open_folder(path, cx);
    }

    pub(crate) fn toggle_sidebar_section(
        &mut self,
        section: SidebarSection,
        cx: &mut Context<Self>,
    ) {
        if !self.sidebar_collapsed.remove(&section) {
            self.sidebar_collapsed.insert(section);
        }
        cx.notify();
    }
}

fn sidebar_section(
    section: SidebarSection,
    title: &'static str,
    meta: String,
    open: bool,
    content: AnyElement,
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
                    "sidebar-section-{}",
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
                .debug_selector(move || format!("sidebar-section-{}", section.key()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_sidebar_section(section, cx);
                }))
                .child(group_label(title))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(micro(meta))
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
    rows: &mut Vec<AnyElement>,
    cx: &mut Context<ContactSheet>,
) {
    let children = folder_children(path);
    let is_expanded = expanded.contains(path);
    let is_selected = selected == Some(path);
    let row_path = path.to_path_buf();
    let chevron = if children.is_empty() {
        div().size(px(18.0)).into_any_element()
    } else {
        let toggle_path = path.to_path_buf();
        let icon_path = if is_expanded {
            assets::CHEVRON_DOWN
        } else {
            assets::CHEVRON_RIGHT
        };
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "folder-toggle-{}",
                path.display()
            ))))
            .size(px(18.0))
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
                this.toggle_folder(toggle_path.clone(), cx);
            }))
            .child(icon(icon_path, 10.0, theme::faint()))
            .into_any_element()
    };
    rows.push(
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "folder-row-{}",
                path.display()
            ))))
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
                this.select_folder(&row_path, cx);
            }))
            .child(chevron)
            .child(icon(assets::FOLDER, 14.0, theme::faint()))
            .child(
                ui_text(
                    theme::TEXT_LABEL,
                    if is_selected {
                        theme::ink()
                    } else {
                        theme::muted()
                    },
                )
                .truncate()
                .child(folder_name(path)),
            )
            .into_any_element(),
    );
    if is_expanded {
        for child in children {
            push_folder_rows(&child, depth + 1, selected, expanded, rows, cx);
        }
    }
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
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "sort-{}",
            key.label().to_lowercase()
        ))))
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if active { theme::ink() } else { theme::rule() })
        .bg(if active {
            theme::accent_soft()
        } else {
            theme::surface()
        })
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_soft()))
        .font_family(theme::FONT_UI)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_size(px(theme::TEXT_LABEL))
        .text_color(if active { theme::ink() } else { theme::muted() })
        .debug_selector(move || format!("sort-{}", key.label().to_lowercase()))
        .on_click(cx.listener(move |this, _, _, cx| this.set_sort_key(key, cx)))
        .child(key.label())
        .into_any_element()
}

fn label_filter_row(label: ColorLabel, count: usize, active: bool) -> gpui::Stateful<gpui::Div> {
    let color = theme::label_color(label);
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "filter-label-{}",
            label.label().to_lowercase()
        ))))
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
        .active(|style| style.opacity(0.8))
        .hover(|style| style.bg(theme::accent_soft()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().size(px(9.0)).bg(color))
                .child(
                    ui_text(theme::TEXT_BODY - 1.0, theme::muted())
                        .child(format!("{} label", label.label())),
                ),
        )
        .child(mono_text(theme::TEXT_META, theme::faint()).child(count.to_string()))
}

fn filter_row(
    id: impl Into<ElementId>,
    name: &str,
    count: usize,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
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
            .items_center()
            .gap(px(8.0))
            .child(div().w(px(3.0)).h(px(3.0)).bg(if active {
                theme::accent()
            } else {
                rgba(0x00000000)
            }))
            .child(
                ui_text(
                    theme::TEXT_BODY,
                    if active { theme::ink() } else { theme::muted() },
                )
                .child(name.to_string()),
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
        .child(format!("{count}")),
    )
}
