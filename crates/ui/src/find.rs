//! The find bar: a browser-style search across every document.
//!
//! Ctrl+F opens a small bar over the top-right of the canvas without
//! covering the page. Results update as you type. Matches in the open
//! document come first, from the current page on, and the first one is shown
//! at once. Enter and Shift+Enter (also F3 and Ctrl+G) step through each
//! match, then on to other pages and documents, wrapping at the end. Every
//! match on the page is marked and the current one stands out. Escape
//! closes the bar and clears the marks.
use super::*;

/// Results follow typing after this pause.
const TYPING_PAUSE: Duration = Duration::from_millis(140);

pub(super) struct Find {
    pub field: Entity<Field>,
    /// The text whose results are shown.
    searched: String,
    /// The latest edit not yet searched.
    typed: Option<Instant>,
    /// The search generation `order` was made for.
    ordered: Option<u64>,
    /// Indices into the controller's results: the open document's pages
    /// from the current one on, then its earlier pages, then other
    /// documents.
    order: Vec<usize>,
    /// Position in `order` of the result shown.
    current: Option<usize>,
    /// Center the current match once its document has loaded.
    reveal: bool,
    _subscriptions: [Subscription; 2],
}

impl NotesView {
    /// Ctrl+F: open the find bar, or select its text when already open.
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            let field = cx.new(|cx| {
                let mut field = Field::new(self.controller.search_query.clone(), false, cx);
                field.placeholder = Some("Find in all documents".into());
                field.compact = true;
                field
            });
            let subscription = cx.observe(&field, |this, field, cx| {
                let text = field.read(cx).content.trim().to_owned();
                if let Some(find) = &mut this.find
                    && text != find.searched
                {
                    find.typed = Some(Instant::now());
                    cx.spawn(async move |view, cx| {
                        cx.background_executor().timer(TYPING_PAUSE).await;
                        let _ = view.update(cx, |view, cx| view.search_typed(cx));
                    })
                    .detach();
                }
            });
            // The field's own Enter binding runs before key listeners.
            let submitted = cx.subscribe(&field, |this, _, _: &Submitted, cx| {
                this.step_find(true, cx)
            });
            // Reopening keeps the last query and its results, as browsers do.
            let searched = self.controller.search_query.clone();
            self.find = Some(Find {
                field,
                searched,
                typed: None,
                ordered: None,
                order: Vec::new(),
                current: None,
                reveal: false,
                _subscriptions: [subscription, submitted],
            });
        }
        let find = self.find.as_ref().unwrap();
        let field = find.field.clone();
        let theme = Theme::new(&self.controller.settings);
        field.update(cx, |field, cx| {
            field.theme = theme;
            field.select_all_content();
            cx.notify();
        });
        field.read(cx).focus.focus(window);
        cx.notify();
    }

    /// Close the find bar, clearing the marks and returning to the page.
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            self.controller.search_highlights.clear();
            self.focus.focus(window);
            cx.notify();
        }
    }

    /// Search for the field's text once typing has paused.
    fn search_typed(&mut self, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        let Some(typed) = find.typed else {
            return;
        };
        if typed.elapsed() < TYPING_PAUSE {
            return;
        }
        find.typed = None;
        let text = find.field.read(cx).content.trim().to_owned();
        if text == find.searched {
            return;
        }
        find.searched = text.clone();
        find.ordered = None;
        find.order.clear();
        find.current = None;
        self.controller.search_highlights.clear();
        self.controller.search(text);
        cx.notify();
    }

    /// Order fresh results and show the first match in the open document.
    fn sync_find(&mut self) {
        let generation = self.controller.search_generation();
        let ready = self.controller.search_state == folio_app::SearchState::Complete;
        let Some(find) = &mut self.find else {
            return;
        };
        if ready && find.ordered != Some(generation) && find.typed.is_none() {
            find.ordered = Some(generation);
            let results = &self.controller.search_results;
            let active = self.controller.active;
            let page = self.controller.session().page + 1;
            let mut here = results
                .iter()
                .enumerate()
                .filter(|(_, r)| r.note == active)
                .map(|(i, r)| (r.page_number.unwrap_or(0), i))
                .collect::<Vec<_>>();
            here.sort();
            let split = here.partition_point(|(number, _)| *number < page);
            here.rotate_left(split);
            find.order = here.into_iter().map(|(_, i)| i).collect();
            find.order.extend(
                results
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.note != active)
                    .map(|(i, _)| i),
            );
            find.current = None;
            // Like a browser, show the first match while typing, but only
            // within the open document; other documents open on Enter.
            let first_here = find
                .order
                .first()
                .is_some_and(|&i| results[i].note == active);
            if first_here && !self.library_open {
                self.go_to_result(0);
            }
        }
        let Some(find) = &mut self.find else {
            return;
        };
        if find.reveal && !self.controller.search_highlights.is_empty() {
            find.reveal = false;
            self.reveal_match();
        }
    }

    /// Show the result at `position` in the find order.
    fn go_to_result(&mut self, position: usize) {
        let Some(find) = &mut self.find else {
            return;
        };
        let Some(&index) = find.order.get(position) else {
            return;
        };
        find.current = Some(position);
        find.reveal = true;
        let result = &self.controller.search_results[index];
        let (note, page) = (result.note, result.page);
        let changed = note != self.controller.active;
        self.controller.navigate_search(note, page);
        if changed || self.library_open {
            self.show_editor();
        }
        if !self.controller.search_highlights.is_empty()
            && let Some(find) = &mut self.find
        {
            find.reveal = false;
            self.reveal_match();
        }
    }

    /// Step to the next or previous match: through the marks on this page,
    /// then to the neighboring result, wrapping around.
    pub(super) fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.sync_find();
        let Some(find) = &self.find else {
            return;
        };
        let count = find.order.len();
        if count == 0 {
            return;
        }
        let marks = self.controller.search_highlights.len();
        let focus = self.controller.search_focus;
        match find.current {
            Some(_) if forward && focus + 1 < marks => {
                self.controller.search_focus += 1;
                self.reveal_match();
            }
            Some(_) if !forward && focus > 0 && marks > 0 => {
                self.controller.search_focus -= 1;
                self.reveal_match();
            }
            Some(position) => {
                let next = if forward {
                    (position + 1) % count
                } else {
                    (position + count - 1) % count
                };
                self.go_to_result(next);
                if !forward
                    && let Some(last) = self.controller.search_highlights.len().checked_sub(1)
                {
                    self.controller.search_focus = last;
                    self.reveal_match();
                }
            }
            None => self.go_to_result(if forward { 0 } else { count - 1 }),
        }
        cx.notify();
    }

    /// Scroll the current match into view when it is outside the canvas or
    /// near its edges.
    fn reveal_match(&mut self) {
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        let Some(rect) = self
            .controller
            .search_highlights
            .get(self.controller.search_focus)
            .copied()
        else {
            return;
        };
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        let viewport = &mut self.controller.session_mut().viewport;
        let corners = [rect.min, rect.max].map(|p| viewport.to_screen(p));
        let margin = 48.;
        let inside = corners.iter().all(|p| {
            p.x >= margin && p.x <= width - margin && p.y >= margin && p.y <= height - margin
        });
        if !inside {
            let center = viewport.to_screen(rect.center());
            viewport.pan.x += width / 2. - center.x;
            viewport.pan.y += height / 2. - center.y;
            self.synchronize_page_view();
        }
    }

    /// The find bar, floating over the top-right corner of the content.
    pub(super) fn find_bar(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.sync_find();
        // Dialogs take over; the bar returns when they close.
        if self.modal.is_some()
            || self.settings_open
            || self.help_open
            || self.controller.error.is_some()
        {
            return None;
        }
        let find = self.find.as_ref()?;
        let theme = Theme::new(&self.controller.settings);
        let field = find.field.clone();
        field.update(cx, |field, _| field.theme = theme);
        let text = field.read(cx).content.trim().to_owned();
        let results = &self.controller.search_results;
        let pending = find.typed.is_some()
            || text != find.searched
            || self.controller.search_state == folio_app::SearchState::Searching;
        let failed = matches!(
            self.controller.search_state,
            folio_app::SearchState::Failed(_)
        );
        let none = !text.is_empty() && !pending && !failed && find.order.is_empty();
        let count = find.order.len();
        let counter = if text.is_empty() {
            String::new()
        } else if pending {
            "…".into()
        } else if failed {
            "Failed".into()
        } else if let Some(position) = find.current {
            format!("{}/{count}", position + 1)
        } else {
            format!("0/{count}")
        };
        // The current match's place, or where Enter goes next.
        let context = match find.current.and_then(|p| find.order.get(p)) {
            Some(&index) if !pending => {
                let result = &results[index];
                let mut place = result.title.clone();
                if let Some(page) = result.page_number {
                    place.push_str(&format!(" · Page {page}"));
                }
                let marks = self.controller.search_highlights.len();
                if marks > 1 {
                    place.push_str(&format!(
                        " · {} of {marks} here",
                        self.controller.search_focus.min(marks - 1) + 1
                    ));
                }
                Some((place, snippet(result.snippet_parts(), theme)))
            }
            None if !pending && count > 0 => {
                let first = &results[find.order[0]];
                Some((
                    if first.note == self.controller.active || self.library_open {
                        format!("{count} matching page{}", if count == 1 { "" } else { "s" })
                    } else {
                        "Not in this document".into()
                    },
                    plain(format!("Enter opens “{}”", first.title)),
                ))
            }
            _ if text.is_empty() && self.library_open => Some((
                "Search every document".into(),
                plain("Titles, tags, text and handwriting".into()),
            )),
            _ if none => Some((
                "No matches".into(),
                plain(
                    "Searches titles, tags, typed text, equations and indexed handwriting".into(),
                ),
            )),
            _ => None,
        };
        // Below the toolbar, at the right edge of the page area.
        let search = self.accessibility.control_bounds_for_key("search");
        let top = search.map_or(px(96.), |r| px(r.y1 as f32 + 10.));
        // In the library, right-aligned under the search button, over the
        // empty middle of the sort row rather than the view switch.
        let right = if self.library_open {
            search.map_or(px(16.), |r| window.viewport_size().width - px(r.x1 as f32))
        } else {
            self.canvas_bounds.map_or(px(16.), |b| {
                window.viewport_size().width - b.origin.x - b.size.width + px(16.)
            })
        };
        let nav = |this: &mut Self,
                   id: &'static str,
                   label: &'static str,
                   kind: Icon,
                   forward: bool,
                   cx: &mut Context<Self>| {
            this.icon_button(id, label, kind, false, cx, move |this, _, cx| {
                this.step_find(forward, cx)
            })
            .size(rems(1.75))
            .p_0()
            // Dimmed until there is a match to step to.
            .when(count == 0, |button| button.opacity(0.35))
        };
        let previous = nav(
            self,
            "find-previous",
            "Previous match · Shift+Enter",
            Icon::Up,
            false,
            cx,
        );
        let next = nav(
            self,
            "find-next",
            "Next match · Enter",
            Icon::Down,
            true,
            cx,
        );
        let close = self
            .icon_button(
                "find-close",
                "Close find · Escape",
                Icon::Close,
                false,
                cx,
                |this, w, cx| this.close_find(w, cx),
            )
            .size(rems(1.75))
            .p_0();
        let bar = div()
            .id("find-bar")
            .key_context("FolioFind")
            .occlude()
            .absolute()
            .top(top)
            .right(right)
            .w(rems(25.))
            .max_w(relative(0.9))
            .p_1p5()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            // Browser keys, handled before the field sees them.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                let shift = key.modifiers.shift;
                let control = key.modifiers.control;
                // Plain Enter and Escape are field bindings; see above and
                // the Escape action below.
                match key.key.as_str() {
                    "enter" if shift && !control => this.step_find(false, cx),
                    "f3" => this.step_find(!shift, cx),
                    "g" if control => this.step_find(!shift, cx),
                    "f" if control => this.open_find(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.close_find(window, cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .child(field)
                            // A field without matches turns red, as in browsers.
                            .when(none || failed, |wrap| {
                                wrap.child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .rounded(px(theme.radius))
                                        .border_1()
                                        .border_color(rgb(theme.destructive)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .when(!text.is_empty(), |counter| counter.min_w(rems(2.75)))
                            .flex()
                            .justify_end()
                            .text_xs()
                            .text_color(rgb(if none || failed {
                                theme.destructive
                            } else {
                                theme.muted
                            }))
                            .child(counter),
                    )
                    .child(previous)
                    .child(next)
                    .child(div().w(px(1.)).h(rems(1.25)).mx_0p5().bg(theme.border))
                    .child(close),
            )
            .children(context.map(|(place, detail)| {
                div()
                    .px_2()
                    .pb_0p5()
                    .flex()
                    .flex_col()
                    .text_xs()
                    .child(div().truncate().text_color(rgb(theme.ink)).child(place))
                    .child(div().truncate().text_color(rgb(theme.muted)).child(detail))
            }));
        Some(bar)
    }
}

/// A result's snippet on one line, with its matches emphasized.
fn snippet(parts: Vec<(String, bool)>, theme: Theme) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for (part, matched) in parts {
        let start = text.len();
        text.extend(
            part.chars()
                .map(|c| if c.is_whitespace() { ' ' } else { c }),
        );
        if matched {
            highlights.push((
                start..text.len(),
                HighlightStyle {
                    color: Some(rgb(theme.ink).into()),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

fn plain(text: String) -> StyledText {
    StyledText::new(text)
}
