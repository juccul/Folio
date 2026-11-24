//! The find bar: a browser-style search across every document.
//!
//! Ctrl+F (or the search button) widens the search button into a bar in
//! place, without covering the page. Results update as you type. Matches in the open
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
    /// When the bar opened: the match details drop in once it has widened.
    opened: Instant,
    /// Opened in the library, where it filters the shelf by title instead
    /// of searching inside documents. Switching views closes it.
    library: bool,
    _subscriptions: [Subscription; 2],
}

impl NotesView {
    /// Ctrl+F: open the find bar, or select its text when already open.
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            let library = self.library_open;
            let field = cx.new(|cx| {
                // The library's filter starts empty each time; the editor's
                // find keeps its last query, as browsers do.
                let query = if library { String::new() } else { self.controller.search_query.clone() };
                let mut field = Field::with_cx(query, false, cx);
                field.placeholder = Some(if library { "Filter documents" } else { "Find in all documents" }.into());
                field.compact = true;
                field.on_escape = Some(std::rc::Rc::new(|this: &mut NotesView, window, cx| {
                    this.close_find(window, cx)
                }));
                field
            });
            let subscription = cx.observe(&field, |this, field, cx| {
                // The library's filter applies as you type, and the shelf
                // animates it: documents that no longer match leave, the
                // rest glide to their new places. (Typing commits would
                // otherwise land at once.)
                if this.find.as_ref().is_some_and(|find| find.library) {
                    cx.animate();
                    cx.notify();
                    return;
                }
                let text = field.read(cx).content().trim().to_owned();
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
                if !this.find.as_ref().is_some_and(|find| find.library) {
                    this.step_find(true, cx)
                }
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
                opened: Instant::now(),
                library,
                _subscriptions: [subscription, submitted],
            });
        }
        let Some(field) = self.find.as_ref().map(|find| find.field.clone()) else {
            return;
        };
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
        let text = find.field.read(cx).content().trim().to_owned();
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
        // The library's filter and the editor's find belong to their views.
        if self.find.as_ref().is_some_and(|find| find.library != self.library_open) {
            self.find = None;
            self.controller.search_highlights.clear();
        }
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
        if find.library {
            return;
        }
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

    /// What the find bar shows for its query: the match counter, whether
    /// nothing matched or the search failed, how many matches there are,
    /// and the current match's place (or where Enter goes next).
    fn find_status(&self, cx: &App) -> Option<FindStatus> {
        let find = self.find.as_ref()?;
        let theme = Theme::new(&self.controller.settings);
        let text = find.field.read(cx).content().trim().to_owned();
        if find.library {
            // The library's filter shows its count under the shelf's title;
            // the bar only turns red when nothing is left.
            let count = self.library_filter(cx).map(|words| {
                self.controller
                    .visible_notes()
                    .into_iter()
                    .filter(|n| title_matches(&n.title, &words))
                    .count()
            });
            return Some(FindStatus {
                none: count == Some(0),
                counter: String::new(),
                text,
                count: 0,
                context: None,
            });
        }
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
                Some((place, Some(snippet(result.snippet_parts(), theme))))
            }
            None if !pending && count > 0 => {
                let first = &results[find.order[0]];
                Some((
                    if first.note == self.controller.active || self.library_open {
                        format!("{count} matching page{}", if count == 1 { "" } else { "s" })
                    } else {
                        "Not in this document".into()
                    },
                    Some(plain(format!("Enter opens “{}”", first.title))),
                ))
            }
            _ if text.is_empty() && self.library_open => {
                Some(("Searches titles, tags, text and handwriting".into(), None))
            }
            _ if none => Some((
                "No matches".into(),
                Some(plain(
                    "Searches titles, tags, typed text, equations and indexed handwriting".into(),
                )),
            )),
            _ => None,
        };
        Some(FindStatus { text, none: none || failed, count, counter, context })
    }

    /// The search button, which a click widens in place into the search
    /// bar: the field, the match count, match navigation and Close. Closed,
    /// it narrows back into the button. `button` is the closed button and
    /// `closed` its width and `open` the bar's, in rems; `here` says whether the bar belongs in this
    /// slot (the library's, or the editor's).
    pub(super) fn search_slot(
        &mut self,
        button: Stateful<Div>,
        closed: f32,
        open: f32,
        here: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        self.sync_find();
        let theme = Theme::new(&self.controller.settings);
        let status = if here { self.find_status(cx) } else { None };
        let field = self.find.as_ref().map(|find| find.field.clone());
        let slot = div()
            .id("search-slot")
            .track_bounds("search-slot")
            .flex_shrink_0()
            .h(rems(2.375))
            .flex()
            .items_center()
            .rounded(px(theme.radius))
            .overflow_hidden()
            // Each edge springs on its own, so the edge beside the other
            // controls stays put while the far one travels.
            .el(|e| {
                e.motion(crate::motion::SETTLE)
                    .lead(crate::motion::SETTLE, crate::motion::SETTLE)
                    .animated_changes()
            });
        let ring = rgb(theme.ring).color();
        let (Some(status), Some(field)) = (status, field) else {
            // The button gives way to the bar at once, and fades back in
            // as the bar narrows into it.
            let button = button.el(|e| {
                e.transition(rune::Transition::opacity().springs(rune::SpringPair::new(
                    crate::motion::SWAP,
                    rune::Spring::NONE,
                )))
            });
            // Its box stays, clear, so closing fades it as it narrows.
            return slot
                .w(rems(closed))
                .bg(transparent_black())
                .border_1()
                .border_color(transparent_black())
                .child(button);
        };
        field.update(cx, |field, _| {
            field.theme = theme;
            field.bare = true;
        });
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
            // As wide as the open slot from the start, so the slot's growth
            // reveals it rather than reflowing it.
            .w_full()
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_0p5()
            .pl_2()
            .pr(rems(0.125))
            // The controls fade in as the slot widens and go at once as it
            // narrows back into the button.
            .el(|e| {
                e.transition(rune::Transition::opacity().springs(rune::SpringPair::new(
                    crate::motion::SWAP,
                    rune::Spring::NONE,
                )))
            })
            // Browser keys, handled before the field sees them.
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
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
            .child(icon(Icon::Search, theme.muted).size(rems(1.)).flex_shrink_0())
            .child(div().flex_1().min_w_0().child(field))
            .child(
                div()
                    .when(!status.text.is_empty(), |counter| counter.min_w(rems(2.5)))
                    .flex()
                    .justify_end()
                    .text_xs()
                    .text_color(rgb(if status.none {
                        theme.destructive
                    } else {
                        theme.muted
                    }))
                    .child(status.counter),
            )
            // Match navigation appears once there is a match.
            .when(status.count > 0, |row| row.child(previous).child(next))
            .child(close);
        // The slot draws the bar's box, so the box widens exactly as the
        // slot does while the controls in it are revealed.
        slot.w(rems(open))
            .bg(theme.raised())
            .border_1()
            // A field without matches turns red, as in browsers.
            .border_color(if status.none { rgb(theme.destructive) } else { theme.input })
            .el(move |e| e.focus_within_ring(1., -1., ring))
            .child(bar)
    }

    /// Where the current match is, dropped under the search bar.
    pub(super) fn find_bar(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.sync_find();
        // Dialogs take over; it returns when they close.
        if self.modal.is_some()
            || self.settings_open
            || self.help_open
            || self.controller.error.is_some()
        {
            return None;
        }
        let theme = Theme::new(&self.controller.settings);
        let (place, detail) = self.find_status(cx)?.context?;
        let slot = self.accessibility.control_bounds_for_key("search-slot")?;
        // It drops in once the bar has mostly widened.
        let wait = Duration::from_millis(170).saturating_sub(self.find.as_ref()?.opened.elapsed());
        if !wait.is_zero() {
            cx.wake_after(wait);
            return None;
        }
        // Both search bars widen to the left, so it hangs from their right.
        let right = f32::from(window.viewport_size().width) - slot.x1 as f32;
        Some(
            div()
                .id("find-context")
                .el(|e| {
                    e.transition(
                        rune::Transition::offset(0., -4.)
                            .combined(rune::Transition::opacity())
                            .springs(rune::SpringPair::new(crate::motion::POP, crate::motion::SETTLE)),
                    )
                })
                .occlude()
                .absolute()
                .top(px(slot.y1 as f32 + 6.))
                .right(px(right))
                .w(px((slot.x1 - slot.x0) as f32))
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .text_size(rems(0.8125))
                .rounded(px(theme.corner_md()))
                .glass(theme, theme.popover)
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.ink))
                        .child(place),
                )
                .children(detail.map(|detail| {
                    div()
                        .truncate()
                        .text_color(rgb(theme.secondary()))
                        .child(detail)
                })),
        )
    }
}

impl NotesView {
    /// The words the library's search filters the shelf by, when it is open
    /// and has any.
    pub(super) fn library_filter(&self, cx: &App) -> Option<Vec<String>> {
        let find = self.find.as_ref().filter(|find| find.library && self.library_open)?;
        let words: Vec<String> = find
            .field
            .read(cx)
            .content()
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        (!words.is_empty()).then_some(words)
    }
}

/// Whether `title` holds every one of `words` (lowercase).
pub(super) fn title_matches(title: &str, words: &[String]) -> bool {
    let title = title.to_lowercase();
    words.iter().all(|word| title.contains(word.as_str()))
}

/// What the find bar shows (see `NotesView::find_status`).
struct FindStatus {
    text: String,
    /// Nothing matched, or the search failed.
    none: bool,
    count: usize,
    counter: String,
    context: Option<(String, Option<StyledText>)>,
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
