//! The text formatting bar's font menu: a dropdown under the font button
//! listing the default family, fonts already used in the document and every
//! installed family, each shown in its own face. Typing filters the list.
use super::{
    Escape, NotesView, Theme,
    field::{Field, Navigate, Submitted},
    icons::{Icon, icon},
    theme::Glass,
};
use folio_document::Object;
use gpui::{prelude::*, *};
use std::rc::Rc;

/// The family text boxes use unless another is chosen. It resolves to the
/// interface's fallback font on screen and to the generic family in exports.
const DEFAULT_FAMILY: &str = "sans-serif";
const ROW_HEIGHT: f32 = 32.;
const VISIBLE_ROWS: usize = 11;

pub(super) struct FontMenu {
    field: Entity<Field>,
    /// The highlighted row, moved by the arrow keys.
    cursor: usize,
    query: String,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
enum Row {
    Heading(&'static str),
    Font(SharedString),
}

impl NotesView {
    /// The font family of the selected text box.
    fn selected_font(&self) -> Option<String> {
        self.controller.session().selection.iter().find_map(|id| {
            match self.controller.page().objects.get(id)?.as_ref() {
                Object::Text(text) => Some(text.font_family.clone()),
                _ => None,
            }
        })
    }
    /// Installed families, read once per run.
    fn installed_fonts(&mut self, cx: &App) -> Rc<Vec<SharedString>> {
        self.fonts
            .get_or_insert_with(|| {
                let mut names = cx
                    .text_system()
                    .all_font_names()
                    .into_iter()
                    // Hidden and interface aliases are not choices.
                    .filter(|name| !name.starts_with('.') && name != DEFAULT_FAMILY)
                    .map(SharedString::from)
                    .collect::<Vec<_>>();
                names.sort_by_key(|name| name.to_lowercase());
                names.dedup();
                Rc::new(names)
            })
            .clone()
    }
    /// Families other text boxes of this document use, besides the default.
    fn document_fonts(&self) -> Vec<SharedString> {
        let mut fonts = self
            .controller
            .session()
            .document
            .pages
            .iter()
            .flat_map(|page| page.objects.values())
            .filter_map(|object| match object.as_ref() {
                Object::Text(text) if text.font_family != DEFAULT_FAMILY => {
                    Some(text.font_family.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        fonts.sort_by_key(|name| name.to_lowercase());
        fonts.dedup();
        fonts.into_iter().map(SharedString::from).collect()
    }
    /// The menu's rows for its query. Without one: the default, fonts in
    /// this document, then all installed fonts. With one: every family
    /// containing all of its words, names starting with it first.
    fn font_rows(&mut self, cx: &App) -> Vec<Row> {
        let query = self
            .font_menu
            .as_ref()
            .map(|menu| menu.query.clone())
            .unwrap_or_default();
        let installed = self.installed_fonts(cx);
        let document = self.document_fonts();
        if query.is_empty() {
            let mut rows = vec![Row::Font(DEFAULT_FAMILY.into())];
            if !document.is_empty() {
                rows.push(Row::Heading("In this document"));
                rows.extend(document.into_iter().map(Row::Font));
            }
            rows.push(Row::Heading("All fonts"));
            rows.extend(installed.iter().cloned().map(Row::Font));
            return rows;
        }
        let words = query.split_whitespace().collect::<Vec<_>>();
        let mut candidates = vec![SharedString::from(DEFAULT_FAMILY)];
        // Fonts from another computer are matched too.
        candidates.extend(
            document
                .into_iter()
                .filter(|font| !installed.contains(font)),
        );
        candidates.extend(installed.iter().cloned());
        let mut matches = candidates
            .into_iter()
            .filter_map(|name| {
                let lower = name.to_lowercase();
                words
                    .iter()
                    .all(|w| lower.contains(w))
                    .then(|| (!lower.starts_with(&query), name))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(rank, _)| *rank);
        matches
            .into_iter()
            .map(|(_, name)| Row::Font(name))
            .collect()
    }
    pub(super) fn toggle_font_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.font_menu.is_some() {
            self.close_font_menu(window, cx);
            return;
        }
        self.dismiss_popovers();
        let theme = Theme::new(&self.controller.settings);
        let field = cx.new(|cx| {
            let mut field = Field::new(String::new(), false, cx);
            field.theme = theme;
            field.compact = true;
            field.placeholder = Some("Search fonts".into());
            field
        });
        let subscriptions = vec![
            cx.observe(&field, |this, field, cx| {
                let query = field.read(cx).content.trim().to_lowercase();
                if let Some(menu) = &mut this.font_menu
                    && menu.query != query
                {
                    menu.query = query;
                    let first = if menu.query.is_empty() {
                        this.current_font_row(cx)
                    } else {
                        0
                    };
                    this.set_font_cursor(first);
                }
                cx.notify();
            }),
            cx.subscribe(&field, |this, _, navigate: &Navigate, cx| {
                this.move_font_cursor(navigate.0, cx)
            }),
            cx.subscribe_in(&field, window, |this, _, _: &Submitted, window, cx| {
                let cursor = this.font_menu.as_ref().map_or(0, |menu| menu.cursor);
                if let Some(Row::Font(family)) = this.font_rows(cx).get(cursor).cloned() {
                    this.apply_font(&family, window, cx);
                }
            }),
        ];
        field.read(cx).focus.focus(window);
        self.font_menu = Some(FontMenu {
            field,
            cursor: 0,
            query: String::new(),
            _subscriptions: subscriptions,
        });
        let current = self.current_font_row(cx);
        self.set_font_cursor(current);
        cx.notify();
    }
    /// The first row showing the selected box's family.
    fn current_font_row(&mut self, cx: &App) -> usize {
        let current = self.selected_font().unwrap_or_default();
        self.font_rows(cx)
            .iter()
            .position(|row| matches!(row, Row::Font(name) if name.as_ref() == current))
            .unwrap_or(0)
    }
    fn set_font_cursor(&mut self, cursor: usize) {
        if let Some(menu) = &mut self.font_menu {
            menu.cursor = cursor;
            // Keep the row above in view, so a heading stays visible.
            self.font_scroll
                .scroll_to_item_strict(cursor.saturating_sub(1), ScrollStrategy::Top);
        }
    }
    /// Move the highlight to the next or previous font, skipping headings.
    fn move_font_cursor(&mut self, down: bool, cx: &mut Context<Self>) {
        let rows = self.font_rows(cx);
        let Some(menu) = &mut self.font_menu else {
            return;
        };
        let count = rows.len();
        if count == 0 {
            return;
        }
        let mut cursor = menu.cursor.min(count - 1);
        for _ in 0..count {
            cursor = if down {
                (cursor + 1) % count
            } else {
                (cursor + count - 1) % count
            };
            if matches!(rows[cursor], Row::Font(_)) {
                break;
            }
        }
        menu.cursor = cursor;
        let strategy = if down {
            ScrollStrategy::Bottom
        } else {
            ScrollStrategy::Top
        };
        // Scrolls only when the row has left the view.
        self.font_scroll.scroll_to_item(cursor, strategy);
        cx.notify();
    }
    pub(super) fn close_font_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.font_menu.take().is_none() {
            return;
        }
        if let Some(editor) = &self.inline_text {
            editor.field.read(cx).focus.focus(window);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }
    /// Use `family` for the selected text boxes and close the menu.
    fn apply_font(&mut self, family: &str, window: &mut Window, cx: &mut Context<Self>) {
        for id in self.controller.session().selection.clone() {
            let family = family.to_owned();
            self.controller.edit_text(id, |t| t.font_family = family);
        }
        self.close_font_menu(window, cx);
    }
    /// The font button of the formatting bar, with its menu when open.
    pub(super) fn font_button(&mut self, family: &str, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let open = self.font_menu.is_some();
        let button = self
            .control(
                "font",
                format!("Font: {family}"),
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_1p5()
                    .w_full()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(SharedString::from(family.to_owned()))
                            .child(family.to_owned()),
                    )
                    .child(
                        icon(if open { Icon::Up } else { Icon::Down }, theme.muted)
                            .size(rems(0.875)),
                    )
                    .into_any_element(),
                open,
                cx,
                |this, window, cx| this.toggle_font_menu(window, cx),
            )
            .w(rems(9.5))
            .h(rems(1.875))
            .min_h_0()
            .px_2()
            .text_xs()
            .rounded(px(theme.radius))
            .border_1()
            .border_color(theme.border);
        let mut anchor = div().relative().flex_shrink_0().child(button);
        if open {
            let menu = self.font_menu_panel(cx);
            let menu = self
                .transitions
                .borrow_mut()
                .overlay("font-menu", self.font_appear, menu);
            anchor = anchor.child(deferred(menu).with_priority(1));
        }
        anchor
    }
    fn font_menu_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let Some(menu) = &self.font_menu else {
            return div();
        };
        let field = menu.field.clone();
        field.update(cx, |field, _| field.theme = theme);
        let query = menu.query.clone();
        let count = self.font_rows(cx).len();
        let list = if count == 0 {
            div()
                .px_2()
                .py_4()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child(format!("No fonts match “{query}”"))
                .into_any_element()
        } else {
            uniform_list(
                "font-menu-list",
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                    let rows = this.font_rows(cx);
                    let mut elements = Vec::new();
                    for index in range {
                        if let Some(row) = rows.get(index).cloned() {
                            elements.push(this.font_row(index, row, cx));
                        }
                    }
                    // Virtual rows register during layout, after the root's render.
                    this.resume_poll(cx);
                    this.accessibility.publish(this, window, cx);
                    elements
                }),
            )
            .track_scroll(self.font_scroll.clone())
            .h(px(ROW_HEIGHT * count.min(VISIBLE_ROWS) as f32))
            .into_any_element()
        };
        div()
            .absolute()
            .left_0()
            .top_full()
            .mt_1()
            .w(rems(17.))
            .occlude()
            // The menu lies outside its anchor, so it closes on presses
            // outside itself; the font button toggles it on its own.
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                let (x, y) = (
                    f32::from(event.position.x) as f64,
                    f32::from(event.position.y) as f64,
                );
                if this
                    .accessibility
                    .control_bounds_for_key("font")
                    .is_some_and(|r| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1)
                {
                    return;
                }
                this.close_font_menu(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.close_font_menu(window, cx)))
            .p_1p5()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .flex()
            .flex_col()
            .gap_1p5()
            .child(div().text_sm().child(field))
            .child(list)
    }
    fn font_row(&mut self, index: usize, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::new(&self.controller.settings);
        let family = match row {
            Row::Heading(title) => {
                return div()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .pb_1()
                    .flex()
                    .items_end()
                    .when(index > 0, |heading| {
                        heading.border_t_1().border_color(theme.border)
                    })
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.muted))
                    .child(title)
                    .into_any_element();
            }
            Row::Font(family) => family,
        };
        let chosen = self.selected_font().as_deref() == Some(family.as_ref());
        let cursor = self.font_menu.as_ref().map_or(0, |menu| menu.cursor);
        let apply = family.clone();
        self.control(
            format!("font-option-{index}"),
            if chosen {
                format!("{family} · Current")
            } else {
                family.to_string()
            },
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .min_w_0()
                .child(div().w(rems(1.)).flex_shrink_0().when(chosen, |slot| {
                    slot.child(icon(Icon::Check, theme.ink).size(rems(1.)))
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .when(!symbol_font(&family), |name| {
                            name.font_family(family.clone())
                        })
                        .child(family.clone()),
                )
                .when(family.as_ref() == DEFAULT_FAMILY, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child("Default"),
                    )
                })
                .into_any_element(),
            index == cursor,
            cx,
            move |this, w, cx| this.apply_font(&apply, w, cx),
        )
        .h(px(ROW_HEIGHT))
        .min_h_0()
        .px_2()
        .justify_start()
        .w_full()
        .into_any_element()
    }
}

/// Whether `family` draws pictures instead of letters, so its name must be
/// shown in the interface font to stay readable.
fn symbol_font(family: &str) -> bool {
    let lower = family.to_lowercase();
    [
        "dingbat", "symbol", "wingding", "webding", "emoji", "icons", "d050000l",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}
