//! Building the window contents each frame.
use super::*;

/// Fade-in opacity of each animated panel for this frame.
struct PanelOpacity {
    modal: f32,
    menu: f32,
    canvas_menu: f32,
    popover: f32,
    tool_menu: f32,
    tool_wheel: f32,
    pages: f32,
    math: f32,
    selection: f32,
    sort: f32,
    font: f32,
}

fn window_title_for(library_open: bool, note_title: &str) -> String {
    if library_open {
        return "Folio".into();
    }
    let trimmed = note_title.trim();
    if trimmed.is_empty() {
        "Folio".into()
    } else {
        format!("{trimmed} — Folio")
    }
}

impl NotesView {
    fn sync_window_title(&mut self, window: &mut Window) {
        let note_title = if self.library_open {
            ""
        } else if self.controller.loading_note()
            && let Some(note) = self
                .controller
                .notes
                .iter()
                .find(|n| n.id == self.controller.requested_note())
        {
            note.title.trim()
        } else {
            self.controller.session().document.metadata.title.trim()
        };
        let unchanged = if self.library_open || note_title.is_empty() {
            self.window_title == "Folio"
        } else {
            self.window_title
                .strip_suffix(" — Folio")
                .is_some_and(|prefix| prefix == note_title)
        };
        if !unchanged {
            self.window_title = window_title_for(self.library_open, note_title);
            window.set_window_title(&self.window_title);
        }
    }
}

impl NotesView {
    pub(crate) fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        self.sync_window_title(window);
        static FIRST_FRAME: std::sync::Once = std::sync::Once::new();
        FIRST_FRAME.call_once(|| super::startup_mark("first_frame"));
        if self.startup_pending > 0 && !self.library_open && !self.controller.loading_note() {
            if self.startup_pending == 2 {
                super::startup_mark("note_loaded");
                self.startup_pending = 1;
            } else if self.painter.ink_settled() {
                super::startup_mark("ink_ready");
                self.startup_pending = 0;
            }
        }
        self.release_hidden_resources(window, cx);
        self.controller.refresh_default_ink();
        self.apply_equation_result(window, cx);
        self.cancel_region_after_navigation();
        self.advance_canvas_ease(window);
        self.advance_canvas_glide();
        if self.controller.select_morph.as_ref().is_some_and(painting::select_morph_done) {
            self.controller.select_morph = None;
        }
        self.building_overlay = false;
        self.accessibility.begin();
        self.motion.borrow_mut().begin_frame();
        self.transitions.borrow_mut().begin_frame();
        self.open_pending_dialogs(window, cx);
        let alpha = self.panel_opacity();
        self.selection_appear = alpha.selection;
        self.sort_appear = alpha.sort;
        self.font_appear = alpha.font;
        self.track_recognition_frame(alpha.modal, window, cx);
        let theme = Theme::new(&self.controller.settings);
        // Build order matters: controls register for accessibility as they are built.
        // The library shows no canvas: its area is built only for documents.
        let center = (!self.library_open).then(|| self.canvas_area(window, cx));
        let mut root = self.root_element(cx);
        if !self.library_open {
            // Painted first, beneath the translucent bars and sidebars.
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgb(theme.desk()))
                    .child(self.canvas_layer(cx)),
            );
        }
        // Keyed, so the editor adding its canvas beneath them does not
        // rebuild (and re-animate) the bars and panels.
        let mut body = div()
            .id("chrome-body")
            // Where the folder menu's layer starts in the window.
            .track_bounds("chrome-body")
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0();
        let mut editor_top = None;
        if self.library_open {
            self.canvas_bounds = None;
            // Read again by the sidebar's rows, which build later this frame.
            let arriving = self.transitions.borrow().switched();
            self.library_arriving = arriving;
            let shelf = self.library(window, cx);
            // Coming back from a document, the sidebar and header are simply
            // there and the documents arrive one after another (they animate
            // themselves), as when the shelf changes filter.
            body = body
                .child(self.library_sidebar(cx))
                .child(shelf.id("library-view").el(move |e| {
                    // Opening a document replaces the shelf at once: a
                    // fading shelf would linger over the arriving canvas.
                    let e = e.transition(rune::Transition::opacity().springs(
                        rune::SpringPair::new(rune::Spring::NONE, rune::Spring::NONE),
                    ));
                    if arriving {
                        e
                    } else {
                        e.animated_changes()
                    }
                }));
        } else {
            let center = center.expect("built when a document is open");
            let (top, main) = self.editor_column(center, &alpha, window, cx);
            editor_top = Some(top);
            body = body.child(main.id("editor-view").el(|e| {
                e.transition(rune::Transition::opacity().springs(rune::SpringPair::new(
                    crate::motion::SETTLE,
                    rune::Spring::NONE,
                )))
                .animated_changes()
            }));
        }
        let canvas_overlay = self.canvas_menu_overlay(&alpha, window, cx);
        let document_menu = self.document_menu_overlay(&alpha, window, cx);
        body = self.add_overlays(body, &alpha, window, cx);
        self.building_overlay = false;
        // One glass bar for the title bar, editor toolbar and notices, so the
        // blurred backdrop is continuous across its rows. Nothing passes
        // behind it in the library, where a blur (whose GPU buffers cost tens
        // of megabytes) would change nothing.
        let bar_theme = if self.library_open {
            Theme {
                glass_blur: 0.,
                ..theme
            }
        } else {
            self.chrome_theme()
        };
        let mut top = div()
            .id("chrome-top")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .glass_fill_from_edge(bar_theme, theme.chrome, true)
            .child(self.document_tabs(window, cx));
        self.store_workspace();
        // The selection toolbar shows recognition's progress when it is on
        // screen; the bar only stands in when it is not.
        if self.controller.recognition_pending && !self.selection_toolbar_shown() {
            top = top.child(self.recognition_notice(cx));
        }
        if let Some(error) = self.controller.save_error.clone() {
            top = top.child(notice_row(format!("Changes are not saved. {error}")).child(
                self.button("retry-save", "Retry saving", false, cx, |this, _, _| {
                    this.controller.retry_save()
                }),
            ));
        }
        let find = self.find_bar(window, cx);
        root = root
            .child(top.children(editor_top))
            .child(body)
            .children(find)
            .child(overlay_layer("document-menu", document_menu))
            .child(canvas_overlay);
        let hosted = self.modal_hosted_in(MenuHost::Popover);
        let popover = (!self.library_open
            && (!self.blocking_overlay() || hosted)
            && (self.more_open || self.pen_settings || self.export_open))
            .then(|| {
                if hosted {
                    self.building_overlay = !self.help_open && self.controller.error.is_none();
                    if self.building_overlay {
                        self.accessibility.begin();
                    }
                }
                let popover = self.popover(window, cx);
                self.transitions
                    .borrow_mut()
                    .overlay("popover", alpha.popover, popover)
            });
        let popover = popover.or_else(|| self.transitions.borrow().closed("popover"));
        root = root.child(overlay_layer("popover", popover));
        root = root.child(
            if self.tool_wheel.is_some() && !self.library_open && !self.blocking_overlay() {
                self.tool_wheel_overlay(alpha.tool_wheel, window, cx)
                    .into_any_element()
            } else {
                super::tool_wheel::tool_wheel_layer().into_any_element()
            },
        );
        let hosted = self.modal_hosted_in(MenuHost::Tool);
        let tool_menu = (self.tool_menu.is_some()
            && !self.library_open
            && (!self.blocking_overlay() || hosted))
            .then(|| {
                if hosted {
                    self.building_overlay = !self.help_open && self.controller.error.is_none();
                    if self.building_overlay {
                        self.accessibility.begin();
                    }
                }
                let menu = self.tool_menu_panel(window, cx);
                self.transitions
                    .borrow_mut()
                    .overlay("tool-menu", alpha.tool_menu, menu)
            });
        let tool_menu = tool_menu.or_else(|| self.transitions.borrow().closed("tool-menu"));
        root = root.child(overlay_layer("tool-menu", tool_menu));
        // The tour fills the whole window, title bar and all (an error
        // shows over the body instead, until it is dismissed).
        let tour = (self.tour.is_some() && self.controller.error.is_none()).then(|| {
            self.building_overlay = true;
            self.accessibility.begin();
            self.tour_panel(window, cx).into_any_element()
        });
        self.building_overlay = false;
        root = root.child(overlay_layer("tour", tour));
        // Overlays that just closed fade out above everything else.
        root = root.children(self.transitions.borrow().exits());
        if self.update_preparing {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .scrim(theme)
                    .flex()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .occlude()
                    .child(
                        div()
                            .p_6()
                            .rounded(px(theme.corner_lg()))
                            .glass_dialog(theme, theme.popover)
                            .text_sm()
                            .child("Saving, backing up and preparing the update…"),
                    ),
            );
        }
        // Menus and dialogs without a field take focus on their first
        // control once it has been built.
        if let Some(key) = self.menu_focus.take() {
            self.accessibility.focus_control(&key, window);
        }
        self.accessibility.publish(self, window, cx);
        titlebar::frame(root, window)
    }
}

impl NotesView {
    /// Advance panel animations. Each panel animates when its key changes,
    /// so switching between two contents of one panel also fades it, and
    /// the content it replaced leaves (see [`transitions`]).
    fn panel_opacity(&self) -> PanelOpacity {
        use transitions::Style;
        let reduced = self.controller.settings.reduce_motion;
        let mut motion = self.motion.borrow_mut();
        let mut transitions = self.transitions.borrow_mut();
        motion.prune();
        if reduced {
            motion.settle();
        }
        // Overlays hidden because the library opened or closed just
        // disappear.
        let animate = !transitions.switch_view(self.library_open) && !reduced;
        let mut panel = |key: &'static str, style: Option<Style>, signature: Option<String>| {
            if let Some(style) = style {
                transitions.observe(
                    key.trim_end_matches("-motion"),
                    style,
                    signature.clone(),
                    animate,
                );
            }
            // Rune animates every panel now; its progress here is 1 (shown)
            // or absent, so the view no longer redraws to tween it.
            let _ = reduced;
            motion.panel(key, signature, true)
        };
        let open = |shown: bool| shown.then(|| "open".to_string());
        let editor = !self.library_open;
        PanelOpacity {
            modal: panel(
                "modal-motion",
                Some(Style::Dialog),
                self.modal.as_ref().map(|(m, _)| m.title().into()),
            ),
            menu: {
                let document = panel(
                    "document-menu-motion",
                    Some(Style::Menu),
                    self.document_menu.map(|(id, _)| id.to_string()),
                );
                let folder = panel(
                    "folder-menu-motion",
                    Some(Style::Menu),
                    self.folder_menu.map(|(id, _)| id.to_string()),
                );
                if self.document_menu.is_some() {
                    document
                } else {
                    folder
                }
            },
            canvas_menu: panel(
                "canvas-menu-motion",
                Some(Style::Menu),
                self.canvas_menu
                    .as_ref()
                    .map(|menu| format!("{:?}", menu.position)),
            ),
            popover: panel(
                "popover-motion",
                Some(Style::Menu),
                (editor
                    && !self.blocking_overlay()
                    && (self.more_open || self.pen_settings || self.export_open))
                    .then(|| {
                        format!(
                            "{}-{}-{}",
                            self.more_open, self.pen_settings, self.export_open
                        )
                    }),
            ),
            tool_menu: panel(
                "tool-menu-motion",
                Some(Style::Menu),
                (editor && !self.blocking_overlay())
                    .then(|| self.tool_menu.map(|(kind, _)| format!("{kind:?}")))
                    .flatten(),
            ),
            tool_wheel: panel(
                "tool-wheel-motion",
                Some(Style::Wheel),
                open(self.tool_wheel.is_some() && editor && !self.blocking_overlay()),
            ),
            pages: panel(
                "pages-motion",
                Some(Style::SlideLeft),
                open(editor && self.pages_open),
            ),
            math: panel(
                "math-motion",
                Some(Style::SlideRight),
                open(editor && self.controller.math_session.is_some()),
            ),
            selection: panel(
                "selection-toolbar-motion",
                Some(Style::Menu),
                open(editor && self.selection_toolbar_shown()),
            ),
            sort: panel(
                "library-sort-menu-motion",
                Some(Style::Menu),
                open(self.library_open && self.sort_open),
            ),
            font: panel(
                "font-menu-motion",
                Some(Style::Menu),
                open(editor && self.font_menu.is_some()),
            ),
        }
    }
    /// Release GPU and decoded resources for views that are not shown.
    fn release_hidden_resources(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.export_open || self.library_open || self.blocking_overlay() {
            self.export_options.clear_preview(window);
        }
        self.release_math_images(window);
        if self.library_open {
            // The library does not paint the editor. Release its GPU images,
            // decoded PDFs and page geometry until the canvas is shown again.
            self.painter.clear(window, cx);
            paint::forget_canvas_runs();
        } else if !matches!(self.modal, Some((Modal::OpenDocument, _))) {
            // The document picker previews covers as the library does; they
            // go once it closes.
            self.controller.clear_library_previews();
            if !self.pages_open {
                self.thumbnails.keep_covers(window);
            }
        }
    }
    /// Close the equation dialog once its submitted LaTeX has rendered, or
    /// show the rendering error.
    fn apply_equation_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.equation_draft.is_none() {
            return;
        }
        let Some(result) = self.controller.equation_result.take() else {
            return;
        };
        let Some(submitted) = self.equation_draft.take() else {
            return;
        };
        match result {
            Ok(()) => {
                if self
                    .modal
                    .as_ref()
                    .is_some_and(|(_, field)| field.read(cx).content() == submitted)
                {
                    self.close_modal(window, cx);
                } else {
                    self.modal_error = Some(
                        "The submitted equation was saved. Your newer draft is still here.".into(),
                    );
                }
            }
            Err(error) => {
                self.modal_error = Some(error);
                if let Some((_, field)) = &self.modal {
                    field.read(cx).focus.focus(window);
                }
            }
        }
    }
    /// Open dialogs requested by the controller: a recognition review for the
    /// visible page, or a PDF password prompt.
    fn open_pending_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_none()
            && !self.library_open
            && !self.settings_open
            && !self.help_open
            && self.controller.interaction.is_none()
            && self
                .controller
                .recognition_review
                .as_ref()
                .is_some_and(|r| {
                    r.note == self.controller.active && r.page == self.controller.page().id
                })
        {
            // Shown in the selection's menu when it is on screen, which
            // turns into the review instead of a second popover opening.
            if !self.library_open && self.selection_toolbar_shown() {
                self.modal_in_menu(MenuHost::Selection, Modal::Recognition, window, cx);
            } else {
                self.modal(Modal::Recognition, window, cx);
            }
        }
        if self.modal.is_none()
            && let Some((note, path)) = self.controller.pending_pdf_password.take()
        {
            self.modal(Modal::PdfPassword(note, path), window, cx);
        }
    }
    /// Report the first frame that shows a recognition result, for OCR timing.
    fn track_recognition_frame(
        &mut self,
        modal_alpha: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let review_visible =
            matches!(self.modal.as_ref(), Some((Modal::Recognition, _))) && modal_alpha > 0.;
        let math_visible = self.controller.math_session.is_some()
            && self.modal.is_none()
            && !self.library_open
            && !self.settings_open
            && !self.help_open;
        if (review_visible || math_visible)
            && let Some(request) = self.controller.recognition_frame_pending()
        {
            let _ = (window, cx);
            self.controller.recognition_frame_completed(request);
        }
    }
    /// The page, painted across the whole editor so that it continues beneath
    /// the glass bars and sidebars. Input, layout and the viewport stay
    /// relative to the visible canvas area, measured by `canvas_element`.
    fn canvas_layer(&mut self, _: &mut Context<Self>) -> Div {
        view_canvas("canvas-layer", |view, bounds, window, cx| {
            // The visible area is measured by the canvas element's layout.
            let Some(area) = view.canvas_bounds else {
                return;
            };
            let editing = view.inline_text.as_ref().map(|e| e.id);
            let viewport = &mut view.controller.session_mut().viewport;
            let pan = viewport.pan;
            viewport.pan.x += f32::from(area.origin.x - bounds.origin.x);
            viewport.pan.y += f32::from(area.origin.y - bounds.origin.y);
            view.painter.hide_selection = view.cropping();
            view.diagnostics.painted();
            view.painter
                .paint(&mut view.controller, bounds, editing, window, cx);
            view.controller.session_mut().viewport.pan = pan;
            view.paint_region(area, window);
        })
    }
    /// The visible document canvas area: measures it and routes pointer input.
    fn canvas_element(&mut self, _: &mut Context<Self>) -> Div {
        let hide_cursor = self.controller.cursor.is_some() && !self.canvas_input_blocked();
        div().id("canvas-input").size_full().el(move |e| {
            let e = e
                .drag_threshold(0.)
                .pointer_buttons([rune::PointerButton::Middle])
                .on_layout_tx(|s: &mut AppState, r: [f32; 4], tx: &mut rune::Tx| {
                    let view = s.view.get_mut();
                    let mut window = Window::render(view);
                    let mut cx = Context::fresh();
                    // The canvas paints only once it knows its size: a new
                    // size (the first layout of an opened page, a resize)
                    // builds the view again at once, rather than waiting for
                    // input (a plain wake during layout is dropped).
                    let before = view.canvas_bounds;
                    view.sync_canvas_bounds(Bounds::from_rect(r), &mut cx);
                    if view.canvas_bounds != before {
                        view.canvas_relaid = true;
                        tx.wake_after(Duration::from_millis(1));
                    }
                    view.queue_effects(window.effects.drain(..).collect());
                    view.queue_effects(cx.effects);
                })
                .on_drag(
                    |s: &mut AppState, drag: &mut rune::Drag, tx: &mut rune::Tx| {
                        let middle = tx.button() == rune::PointerButton::Middle;
                        dispatch(s, tx, |view, window, cx| {
                            let Some(bounds) = view.canvas_bounds else {
                                return;
                            };
                            let position =
                                point(bounds.origin.x + px(drag.x), bounds.origin.y + px(drag.y));
                            view.mouse_position = position;
                            let phase = match drag.phase {
                                rune::Phase::Down => Phase::Down,
                                rune::Phase::Move => Phase::Move,
                                rune::Phase::Up if drag.cancelled() => Phase::Cancel,
                                rune::Phase::Up => Phase::Up,
                            };
                            let button = if middle {
                                MouseButton::Middle
                            } else {
                                MouseButton::Left
                            };
                            view.mouse(position, phase, button, window, cx);
                        });
                    },
                )
                .on_pointer_move_local(
                    |s: &mut AppState, p: rune::PointerEvent, tx: &mut rune::Tx| {
                        let position = point(px(p.window_x), px(p.window_y));
                        dispatch(s, tx, |view, window, cx| {
                            view.mouse_position = position;
                            if view.controller.interaction.is_none() {
                                view.mouse(position, Phase::Hover, MouseButton::Left, window, cx);
                            }
                        });
                    },
                )
                .on_hover_change(|s: &mut AppState, inside: bool, tx: &mut rune::Tx| {
                    if !inside {
                        let position = s.view.get_mut().mouse_position;
                        dispatch(s, tx, |view, window, cx| {
                            if view.controller.interaction.is_none() {
                                view.mouse(position, Phase::Leave, MouseButton::Left, window, cx);
                            }
                        });
                    }
                })
                .on_secondary_press(
                    |s: &mut AppState, press: rune::PointerPress, tx: &mut rune::Tx| {
                        let position = point(px(press.x), px(press.y));
                        dispatch(s, tx, |view, window, cx| {
                            view.mouse_position = position;
                            view.open_canvas_menu(position, window, cx);
                        });
                    },
                )
                .on_wheel(|s: &mut AppState, w: rune::WheelEvent, tx: &mut rune::Tx| {
                    let position = s.view.get_mut().mouse_position;
                    let event = ScrollWheelEvent::from_rune(&w, position);
                    dispatch(s, tx, |view, _, cx| view.scroll(&event, cx));
                    true
                })
                .on_magnify(
                    |s: &mut AppState, m: rune::MagnifyEvent, tx: &mut rune::Tx| {
                        dispatch(s, tx, |view, _, cx| {
                            match m.phase {
                                rune::GesturePhase::Began => view.pinching = true,
                                rune::GesturePhase::Ended => view.pinching = false,
                                rune::GesturePhase::Moved => {}
                            }
                            if !view.canvas_gesture_allowed() {
                                return;
                            }
                            // Off macOS Ctrl + wheel arrives as a magnify of `-dy / 200`,
                            // which a fast wheel takes below -1; an exponent keeps the zoom
                            // positive, as GPUI's Ctrl + wheel zoom did.
                            let delta = m.delta.clamp(-2., 2.);
                            let anchor = DocPoint::new(m.x, m.y);
                            // That Ctrl + wheel has no phases, unlike a touchpad pinch.
                            // It zooms opposite to Rune's mapping and eases into place
                            // as a plain wheel scroll does.
                            let wheel = !view.pinching && m.phase == rune::GesturePhase::Moved;
                            let delta = if wheel { -delta } else { delta };
                            if wheel && !view.controller.settings.reduce_motion {
                                view.ease_zoom(delta, anchor);
                                cx.notify();
                                return;
                            }
                            view.canvas_ease = CanvasEase::default();
                            let factor = delta.exp();
                            view.controller
                                .session_mut()
                                .viewport
                                .zoom_at(factor, anchor);
                            view.synchronize_page_view_zoom();
                            cx.notify();
                        });
                        true
                    },
                )
                .on_rotate(
                    |s: &mut AppState, r: rune::RotateEvent, tx: &mut rune::Tx| {
                        dispatch(s, tx, |view, _, cx| {
                            if !view.canvas_gesture_allowed() || !r.degrees.is_finite() {
                                return;
                            }
                            view.canvas_ease = CanvasEase::default();
                            // Turn about the fingers: the document point under them stays.
                            let anchor = DocPoint::new(r.x, r.y);
                            let viewport = &mut view.controller.session_mut().viewport;
                            let original = viewport.to_document(anchor);
                            viewport.rotation -= r.degrees.to_radians();
                            let after = viewport.to_screen(original);
                            viewport.pan.x += anchor.x - after.x;
                            viewport.pan.y += anchor.y - after.y;
                            view.synchronize_page_view_zoom();
                            cx.notify();
                        });
                        true
                    },
                );
            if hide_cursor {
                e.cursor(rune::Cursor::Hidden)
            } else {
                e
            }
        })
    }
    /// Moves the page along its sideways glide, easing in and out
    /// (smootherstep) so it sets off from rest; a gesture or
    /// leaving the document lands it at once.
    fn advance_canvas_glide(&mut self) {
        let Some(mut glide) = self.canvas_glide else {
            return;
        };
        let k = (glide.start.elapsed().as_secs_f32() / super::CanvasGlide::SECONDS).min(1.);
        let k = if self.controller.interaction.is_some() || self.library_open {
            1.
        } else {
            k
        };
        let want = glide.total * k * k * k * (k * (6. * k - 15.) + 10.);
        self.controller.session_mut().viewport.pan.x += want - glide.applied;
        glide.applied = want;
        self.canvas_glide = (k < 1.).then_some(glide);
    }
    /// Keep the page anchored when the canvas is laid out at a new size or
    /// position, and keep the inline text editor aligned with its text box.
    fn sync_canvas_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let properties = &self.controller.page().properties;
        let viewport = self.controller.session().viewport;
        if !properties.infinite
            && viewport.zoom == 1.
            && viewport.rotation == 0.
            && viewport.pan == DocPoint::new(56., 36.)
        {
            let x = (f32::from(bounds.size.width) - properties.width) / 2.;
            self.controller.session_mut().viewport.pan.x = x;
        } else if let Some(previous) = self.canvas_bounds
            && previous.size.width != bounds.size.width
            && self.controller.interaction.is_none()
        {
            let shift = f32::from(bounds.size.width - previous.size.width) / 2.;
            // A sidebar opening or closing changes the width in one step:
            // the page glides to its new centre alongside it rather than
            // jumping there. A sidebar on the left also moves the canvas's
            // left edge; the page first stays where it was on screen, then
            // glides the whole way. Resizing the window steps finely and
            // follows at once.
            if shift.abs() >= 24. && !self.controller.settings.reduce_motion {
                let moved = f32::from(bounds.origin.x - previous.origin.x);
                self.controller.session_mut().viewport.pan.x -= moved;
                let left = self.canvas_glide.map_or(0., |glide| glide.total - glide.applied);
                self.canvas_glide = Some(super::CanvasGlide {
                    total: left + shift + moved,
                    applied: 0.,
                    start: std::time::Instant::now(),
                });
            } else {
                self.controller.session_mut().viewport.pan.x += shift;
            }
        }
        let canvas_document = (self.controller.active, self.controller.page().id);
        if let Some(previous) = self.canvas_bounds
            && previous.origin.y != bounds.origin.y
            && self.canvas_document.is_some_and(|(note, page)| {
                note == canvas_document.0
                    && (page == canvas_document.1
                        || self.controller.interaction.is_some()
                        || self.controller.session().viewport.pan.y != 36.)
            })
        {
            let delta = f32::from(bounds.origin.y - previous.origin.y);
            self.controller.session_mut().viewport.pan.y -= delta;
            if let Some(Interaction::Pan { pan, .. }) = &mut self.controller.interaction {
                pan.y -= delta;
            }
            if let Some(start) = &mut self.gesture_start {
                start.pan.y -= delta;
            }
            cx.notify();
        }
        if let Some(editor) = &self.inline_text {
            let viewport = self.controller.session().viewport;
            editor.field.update(cx, |field, _| {
                if let Some(style) = &mut field.inline {
                    let text = &style.text;
                    style.transform = folio_document::Transform::translate(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                    )
                    .compose(viewport.transform())
                    .compose(text.transform)
                    .compose(folio_document::Transform::translate(
                        text.rect.min.x,
                        text.rect.min.y,
                    ));
                    style.mask = bounds;
                }
            });
        }
        self.controller.view_size =
            DocPoint::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
        self.canvas_bounds = Some(bounds);
        self.canvas_document = Some(canvas_document);
    }
    /// The canvas with the overlays that sit on it: inline text editing, the
    /// loading notice and the selection toolbar.
    fn canvas_area(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let canvas = self.canvas_element(cx);
        let mut center = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(canvas);
        if let Some(editor) = self.inline_element(cx) {
            center = center.child(editor);
        }
        if self.controller.loading_note() && !self.library_open {
            center = center.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(rgb(theme.canvas.paper))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .text_color(rgb(theme.muted))
                            .child(icon(Icon::Book, theme.muted))
                            .child("Opening document…"),
                    ),
            );
        }
        if !self.library_open && self.selection_toolbar_shown() {
            let hosted = self.modal_hosted_in(MenuHost::Selection);
            if hosted {
                self.building_overlay = !self.help_open && self.controller.error.is_none();
                if self.building_overlay {
                    self.accessibility.begin();
                }
            }
            // Turning into a page and back, the menu's new size moves it:
            // it springs there with its box, so the edge by the selection
            // stays put. Otherwise it follows the selection at once.
            if hosted != self.selection_hosted_was {
                self.selection_hosted_was = hosted;
                self.selection_glide_until = Some(Instant::now() + Duration::from_millis(500));
            }
            self.transitions.borrow_mut().selection_glides =
                self.selection_glide_until.is_some_and(|until| Instant::now() < until);
            let toolbar = self.selection_toolbar(window, cx);
            center = center.child(self.transitions.borrow_mut().overlay(
                "selection-toolbar",
                self.selection_appear,
                toolbar,
            ));
        } else if !self.library_open
            && !self.canvas_input_blocked()
            && self.controller.interaction.is_none()
            && self.inline_text.is_none()
            && !self.cropping()
            && let Some(info) = self.controller.hovered_auto_math_answer()
        {
            center = center.child(self.auto_math_hover_popup(info));
        }
        center
    }
    /// Compact glass popup showing the detected LaTeX for a hovered auto-completed answer.
    fn auto_math_hover_popup(
        &self,
        info: folio_app::AutoMathHoverInfo,
    ) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let (canvas_w, canvas_h) = self
            .canvas_bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((1000., 700.));
        let max_inner_w = (canvas_w - 48.).clamp(120., 380.);

        let (main_w, main_h) = if let Some(ref vec) = info.vector {
            let natural_h = (vec.height * 1.15).clamp(24., 44.);
            let w = (vec.width / vec.height.max(1.) * natural_h).clamp(64., max_inner_w);
            let h = (vec.height / vec.width.max(1.) * w).clamp(20., natural_h);
            (w, h)
        } else {
            (
                (info.detected_latex.len() as f32 * 8.0).clamp(64., max_inner_w),
                22.,
            )
        };

        let ctx_dims = info.context_latex.as_ref().map(|ctx| {
            if let Some(ref vec) = info.context_vector {
                let natural_h = (vec.height * 0.85).clamp(15., 24.);
                let w = (vec.width / vec.height.max(1.) * natural_h).clamp(56., max_inner_w);
                let h = (vec.height / vec.width.max(1.) * w).clamp(14., natural_h);
                (w, h)
            } else {
                ((ctx.len() as f32 * 6.5).clamp(56., max_inner_w), 16.)
            }
        });

        let inner_w = ctx_dims.map_or(main_w, |(cw, _)| main_w.max(cw));
        let popup_w = inner_w + 24.;
        let popup_h = 16. + main_h + ctx_dims.map_or(0., |(_, ch)| ch + 6.);

        let r = info.anchor_bounds;
        let viewport = self.controller.session().viewport;
        let screen = folio_document::Rect::from_points(
            [
                r.min,
                DocPoint::new(r.max.x, r.min.y),
                r.max,
                DocPoint::new(r.min.x, r.max.y),
            ]
            .map(|p| viewport.to_screen(p)),
        );
        let left = (screen.center().x - popup_w * 0.5).clamp(12., (canvas_w - popup_w - 12.).max(12.));
        let above_y = screen.min.y - popup_h - 10.;
        let top = if above_y >= 12. {
            above_y
        } else {
            (screen.max.y + 10.).min((canvas_h - popup_h - 12.).max(12.))
        };

        let mut card = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(popup_w))
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .shadow_md();

        if let Some(vec) = info.vector {
            card = card.child(
                div()
                    .w(px(main_w))
                    .child(self.native_formula(vec, main_h, theme.ink)),
            );
        } else {
            card = card.child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.ink))
                    .child(info.detected_latex),
            );
        }

        if let (Some(ctx), Some((cw, ch))) = (info.context_latex, ctx_dims) {
            if let Some(cvec) = info.context_vector {
                card = card.child(
                    div()
                        .w(px(cw))
                        .pt_0p5()
                        .border_t_1()
                        .border_color(theme.glass_edge())
                        .child(self.native_formula(cvec, ch, theme.muted)),
                );
            } else {
                card = card.child(
                    div()
                        .pt_0p5()
                        .border_t_1()
                        .border_color(theme.glass_edge())
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(ctx),
                );
            }
        }

        card
    }
    /// Whether the selected objects show their action toolbar.
    pub(super) fn selection_toolbar_shown(&self) -> bool {
        !self.controller.read_only()
            && !self.controller.session().selection.is_empty()
            && self.inline_text.is_none()
            && !self.cropping()
    }
    /// The window root with its key context and keyboard actions.
    fn root_element(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        div()
            .id("folio-root")
            .key_context(if self.update_preparing {
                "FolioUpdating"
            } else if self.canvas_menu.is_some() {
                "FolioCanvasMenu"
            } else if self.blocking_overlay() || self.controller.loading_note() {
                "FolioDialog"
            } else if self.library_open {
                "FolioLibrary"
            } else if self.cropping() {
                "Folio FolioCrop"
            } else {
                "Folio"
            })
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.ink))
            .font_family("Noto Sans")
            .on_action(cx.listener(|this, _: &Undo, w, cx| this.step_history(false, w, cx)))
            .on_action(cx.listener(|this, _: &Redo, w, cx| this.step_history(true, w, cx)))
            .on_action(cx.listener(|this, _: &Save, _, cx| {
                this.controller.save();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ResetTitlebarSmoke, _, cx| {
                if this.titlebar_smoke {
                    this.update_smoke_phase(0);
                    this.show_editor();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| {
                // Same dialog as the New document button; Enter accepts the
                // defaults.
                this.modal(Modal::NewDocument, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewPage, _, cx| {
                this.add_page();
                this.show_editor();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Library, _, cx| {
                this.show_library();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TogglePages, _, cx| {
                this.pages_open = !this.pages_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Search, w, cx| this.open_find(w, cx)))
            .on_action(cx.listener(|this, _: &FindNext, _, cx| this.step_find(true, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.step_find(false, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &Paste, window, cx| this.paste(window, cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.controller.select_all();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                this.controller.delete_selection();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Escape, w, cx| this.escape(w, cx)))
            .on_action(cx.listener(|this, _: &Pen, _, cx| {
                // The pen shortcut leaves the highlighter for the last writing style.
                if this.controller.style.tool == InkTool::Highlighter {
                    this.controller
                        .set_style(this.writing_style.take().unwrap_or_default());
                }
                this.select_tool(Tool::Pen);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Eraser, _, cx| {
                this.select_tool(Tool::Eraser);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ApplyCrop, _, cx| this.apply_crop(cx)))
            .on_action(cx.listener(|this, _: &OpenToolWheel, window, cx| {
                // Holding W repeats the key; only a new press toggles.
                if !this.wheel_key_held {
                    this.wheel_key_held = true;
                    this.toggle_tool_wheel(window.mouse_position(), window, cx);
                }
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                if event.keystroke.key == "w" {
                    this.wheel_key_held = false;
                }
            }))
            .on_action(cx.listener(|this, _: &EditText, window, cx| {
                if !this.controller.read_only() {
                    this.edit_selected_text(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleBold, window, cx| {
                this.format_text_shortcut(inline_text::TextFormat::Bold, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleItalic, window, cx| {
                this.format_text_shortcut(inline_text::TextFormat::Italic, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleUnderline, window, cx| {
                this.format_text_shortcut(inline_text::TextFormat::Underline, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Lasso, _, cx| {
                // L returns to the selection tool last chosen, as the toolbar does.
                this.select_tool(this.selection_tool);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Hand, _, cx| {
                this.select_tool(Tool::Hand);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Text, _, cx| {
                this.select_tool(Tool::Text);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Shapes, _, cx| {
                this.select_tool(Tool::Shape);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                this.zoom(1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                this.zoom(1. / 1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FitPage, _, cx| {
                this.fit_eased();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                let p = this.controller.session().page;
                this.go_to_page(p.saturating_sub(1));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                let p = this.controller.session().page;
                this.go_to_page(p + 1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Settings, w, cx| {
                this.open_settings(w);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CloseTab, w, cx| {
                if !this.library_open {
                    this.close_document_tab(this.controller.requested_note(), w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, w, cx| this.cycle_document_tab(true, w, cx)))
            .on_action(
                cx.listener(|this, _: &PreviousTab, w, cx| this.cycle_document_tab(false, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenTab, w, cx| this.modal(Modal::OpenDocument, w, cx)),
            )
            .on_action(cx.listener(|this, _: &Import, _, cx| {
                this.dismiss_popovers();
                this.import(cx);
            }))
            .on_action(cx.listener(|this, _: &Export, _, cx| {
                this.dismiss_popovers();
                this.export_open = true;
                cx.notify();
            }))
    }
    /// Switch tools from a keyboard shortcut or the toolbar. Leaving the
    /// selection tools clears the selection and its toolbars.
    /// Ctrl+B, I and U style the text box being edited or selected text boxes.
    fn format_text_shortcut(
        &mut self,
        format: inline_text::TextFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_text = self.controller.session().selection.iter().any(|id| {
            matches!(
                self.controller.page().objects.get(id).map(|o| o.as_ref()),
                Some(folio_document::Object::Text(_))
            )
        });
        if selected_text
            && !self.blocking_overlay()
            && !self.library_open
            && !self.controller.read_only()
        {
            self.format_text(format, window, cx);
        }
    }
    pub(super) fn select_tool(&mut self, tool: Tool) {
        self.dismiss_popovers();
        self.region_selection = None;
        if !matches!(tool, Tool::Lasso | Tool::Rectangle) {
            self.controller.session_mut().selection.clear();
        }
        self.controller.set_tool(tool);
    }
    /// Escape closes the topmost layer only: an error, help, a dialog,
    /// settings, a menu or popover, then cancels editing state.
    pub(crate) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.controller.error.is_some() && self.controller.interaction.is_none() {
            self.controller.error = None;
        } else if self.tour.is_some() {
            self.close_tour(window);
        } else if self.help_open {
            self.help_open = false;
        } else if self.modal.is_some() {
            self.cancel_modal(window, cx);
            return;
        } else if self.settings_open {
            self.settings_open = false;
        } else if self.folder_menu.is_some() || self.document_menu.is_some() {
            self.folder_menu = None;
            self.document_menu = None;
        } else if self.canvas_menu.is_some() {
            self.canvas_menu = None;
        } else if self.tool_wheel.is_some() {
            self.tool_wheel = None;
        } else if self.font_menu.is_some() {
            self.close_font_menu(window, cx);
            return;
        } else if self.tool_menu.is_some() || self.sort_open {
            self.tool_menu = None;
            self.sort_open = false;
        } else if self.more_open || self.pen_settings || self.export_open {
            self.dismiss_popovers();
        } else if self.find.is_some() {
            self.close_find(window, cx);
            return;
        } else if self.region_selection.take().is_some() {
            self.controller.status = "Region selection cancelled".into();
        } else if self.inline_text.is_some() {
            self.finish_inline_text(window, cx);
            return;
        } else if self.controller.math_session.is_some()
            && self.controller.session().selection.is_empty()
            && self.controller.interaction.is_none()
        {
            self.controller.close_math_solver();
            self.clear_math_inputs(window);
        } else {
            if self.controller.recognition_pending {
                self.controller.cancel_recognition();
            }
            self.controller.cancel();
            self.controller.clear_selection();
        }
        self.focus.focus(window);
        cx.notify();
    }
    /// Undo or redo: library organization changes in the library, document
    /// edits in the editor.
    pub(super) fn step_history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.library_open {
            if !redo {
                self.controller.undo_library();
            }
        } else {
            self.finish_inline_text(window, cx);
            if redo {
                self.controller.redo();
            } else {
                self.controller.undo();
            }
        }
        cx.notify();
    }
    /// The editor: header, notices, the page canvas with its side panels, and footer.
    /// The editor: the header and notices for the top glass bar, and the page
    /// canvas with its side panels and footer.
    fn editor_column(
        &mut self,
        center: Div,
        alpha: &PanelOpacity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, Div) {
        let theme = Theme::new(&self.controller.settings);
        let header = self.editor_header(window, cx);
        let formatting = if self.controller.read_only() {
            None
        } else {
            self.text_formatting_bar(cx)
        };
        let stale_index_notice = self.stale_index_notice(cx);
        let import_notice = self.import_notice(cx);
        let trash_notice = self.trash_notice(cx);
        let footer = self.footer(cx);
        let preview_notice = self.preview_notice(cx);
        let mut workspace = div().relative().flex().flex_1().min_h_0().min_w_0();
        if self.pages_open {
            // Slides in from the left edge without moving the page.
            let panel = div().flex().h_full().child(self.pages_panel(cx));
            workspace = workspace.child(self.transitions.borrow_mut().overlay(
                "pages",
                alpha.pages,
                panel,
            ));
        }
        workspace = workspace.child(center);
        if self.controller.math_session.is_some() {
            let floating =
                f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 1100.;
            let panel = self.math_panel(window, cx).when(floating, |panel| {
                panel
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .border_l_1()
                    .border_color(theme.glass_edge())
            });
            // Slides in from the right edge.
            workspace = workspace.child(
                self.transitions
                    .borrow_mut()
                    .overlay("math", alpha.math, panel),
            );
        }
        let cropping = self.cropping();
        let region_notice = self.region_selection.is_some().then(|| {
            notice_bar()
                .child(if cropping {
                    "Drag the handles, move the box or draw a new one"
                } else {
                    "Drag a rectangle on the page"
                })
                .when(cropping, |bar| {
                    bar.child(self.button(
                        "apply-crop",
                        "Apply crop · Enter",
                        false,
                        cx,
                        |this, _, cx| this.apply_crop(cx),
                    ))
                })
                .child(
                    self.button("cancel-region", "Cancel · Esc", false, cx, |this, _, _| {
                        this.region_selection = None;
                        this.controller.status = "Region selection cancelled".into();
                    }),
                )
        });
        let input_check = self
            .diagnostics
            .check
            .is_some()
            .then(|| self.input_check_panel(cx));
        let top = div()
            .flex()
            .flex_col()
            .child(header)
            .children(trash_notice)
            .children(import_notice)
            .children(stale_index_notice)
            .children(region_notice)
            .children(input_check)
            .children(formatting)
            .children(preview_notice);
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(workspace)
            .child(footer);
        (top, main)
    }
    fn trash_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        self.controller.read_only().then(|| {
            notice_row("In Trash · This document is read only")
                .child(self.button(
                    "restore-document",
                    "Restore document",
                    false,
                    cx,
                    |this, _, _| {
                        this.controller
                            .manage_note(this.controller.active, folio_app::NoteAction::Trash);
                        this.controller.set_tool(Tool::Pen);
                    },
                ))
                .child(
                    self.button("view-trash", "View Trash", false, cx, |this, _, _| {
                        this.controller.filter = NoteFilter::Trash;
                        this.show_library();
                    }),
                )
        })
    }
    /// Progress or failure of importing a file into the active document.
    fn import_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let id = self.controller.active;
        let import = self.controller.library_imports.get(&id)?.clone();
        let name = import
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if import.pending {
            return Some(notice_row(format!("Importing {name}…")));
        }
        let message = format!(
            "Could not import {name}: {}",
            import.error.unwrap_or_default()
        );
        Some(
            notice_row(message)
                .child(self.button(
                    "retry-library-import",
                    "Retry import",
                    false,
                    cx,
                    move |this, _, _| this.controller.retry_library_import(id),
                ))
                .child(self.button(
                    "dismiss-library-import",
                    if self.controller.import_is_provisional(id) {
                        "Remove failed import"
                    } else {
                        "Keep document"
                    },
                    false,
                    cx,
                    move |this, _, _| this.controller.dismiss_failed_import(id),
                )),
        )
    }
    /// Indexed handwriting that changed after its text was reviewed.
    fn stale_index_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let count = self.controller.stale_handwriting_regions();
        if count == 0 {
            return None;
        }
        let plural = if count == 1 { "" } else { "s" };
        Some(
            notice_row(format!(
                "Handwriting changed in {count} indexed region{plural}. Reviewed text is retained and needs updating."
            ))
            .child(self.button(
                "review-stale-index",
                "Review retained text",
                false,
                cx,
                |this, w, cx| {
                    this.controller.review_stale_handwriting();
                    this.modal(Modal::Recognition, w, cx);
                },
            ))
            .when(!self.controller.read_only(), |row| {
                row.child(self.button(
                    "reindex-stale-page",
                    "Recognize page again",
                    false,
                    cx,
                    |this, _, _| {
                        if let Err(error) = this.controller.index_page_handwriting() {
                            this.controller.status = error;
                        }
                    },
                ))
            }),
        )
    }
    fn preview_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let message = self.controller.page_preview_error()?;
        Some(
            notice_row(format!("Preview unavailable: {message}")).child(self.button(
                "retry-page-preview",
                "Retry preview",
                false,
                cx,
                |this, _, _| this.controller.retry_previews(),
            )),
        )
    }
    /// The canvas context menu, drawn over everything with a click-away backdrop.
    /// The canvas context menu's layer: always there, so the menu's exit
    /// plays; while the menu is open it blocks the page and a press outside
    /// the menu closes it.
    fn canvas_menu_overlay(
        &mut self,
        alpha: &PanelOpacity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.canvas_menu.is_some();
        let mut layer = div().id("layer-canvas-menu").absolute().inset_0();
        if open {
            self.building_overlay = true;
            self.accessibility.begin();
            let menu = self.canvas_menu_panel(window, cx);
            layer = layer
                .occlude()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, window, cx| {
                        this.canvas_menu = None;
                        this.focus.focus(window);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.canvas_menu = None;
                        this.focus.focus(window);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(self.transitions.borrow_mut().overlay(
                    "canvas-menu",
                    alpha.canvas_menu,
                    menu,
                ));
        } else {
            layer = layer.children(self.transitions.borrow().closed("canvas-menu"));
        }
        layer
    }
    /// The document menu, laid over the whole window because it opens at a
    /// pointer or anchor position in window coordinates.
    fn document_menu_overlay(
        &mut self,
        alpha: &PanelOpacity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.document_menu.is_none() {
            return self.transitions.borrow().closed("document-menu");
        }
        self.building_overlay = (self.modal.is_none() || self.modal_hosted_in(MenuHost::Document))
            && !self.settings_open
            && !self.help_open
            && self.controller.error.is_none();
        if self.building_overlay {
            self.accessibility.begin();
        }
        let menu = self.document_menu_panel(window, cx);
        Some(
            self.transitions
                .borrow_mut()
                .overlay("document-menu", alpha.menu, menu),
        )
    }
    /// The layer of the menu-like popover `key`: it grows in while
    /// `popover` is there and shrinks back into its opener once it is not.
    fn popover_layer(&self, key: &'static str, popover: Option<AnyElement>) -> Div {
        let shown = match popover {
            Some(p) => Some(self.transitions.borrow_mut().overlay(key, 1., p)),
            None => self.transitions.borrow().closed(key),
        };
        overlay_layer(key, shown)
    }
    /// Menus, panels and dialogs layered over the body. Each layer that is
    /// built while no later layer covers it starts a new accessibility group,
    /// so only the topmost layer is reachable.
    fn add_overlays(
        &mut self,
        mut body: Div,
        alpha: &PanelOpacity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        if self.folder_menu.is_some() {
            self.building_overlay = (self.modal.is_none() || self.modal_hosted_in(MenuHost::Folder))
                && !self.settings_open
                && !self.help_open
                && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            let menu = self.folder_menu_panel(window, cx);
            body = body.child(overlay_layer(
                "folder-menu",
                Some(self.transitions.borrow_mut().overlay("folder-menu", alpha.menu, menu)),
            ));
        } else {
            let closed = self.transitions.borrow().closed("folder-menu");
            body = body.child(overlay_layer("folder-menu", closed));
        }
        // Dialogs sit in layers that stay mounted, so their scrims fade
        // their blur in and out (see `Transitions::dialog_overlay`).
        let settings = self.settings_open.then(|| {
            self.building_overlay =
                self.modal.is_none() && !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            self.settings_panel(window, cx).into_any_element()
        });
        body = body.child(transitions::Transitions::dialog_overlay("settings", theme, settings));
        // Most dialogs and Help are popovers that grow out of what opened
        // them and shrink back into it, as menus do; confirmations stay
        // dialogs over a scrim.
        let modal_menu = self.modal_is_menu();
        // A dialog shown inside the menu it was chosen from is drawn there.
        let modal = (self.modal.is_some() && !self.modal_hosted()).then(|| {
            self.building_overlay = !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            self.modal_panel(false, window, cx).into_any_element()
        });
        let (dialog, popover) = if modal_menu { (None, modal) } else { (modal, None) };
        body = body.child(transitions::Transitions::dialog_overlay("modal", theme, dialog));
        body = body.child(self.popover_layer("modal-menu", popover));
        let help = self.help_open.then(|| {
            self.building_overlay = self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            self.help_panel(window, cx).into_any_element()
        });
        body = body.child(self.popover_layer("help-menu", help));
        if let Some(error) = self.controller.error.clone()
            && self.controller.interaction.is_none()
        {
            self.building_overlay = true;
            self.accessibility.begin();
            body = body.child(
                div()
                    .occlude()
                    .absolute()
                    .inset_0()
                    .scrim(theme)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(530.))
                            .p_6()
                            .glass_dialog(theme, theme.popover)
                            .rounded(px(theme.corner_lg()))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Folio needs your attention"),
                            )
                            .child(div().text_sm().child(error))
                            .child(self.button(
                                "dismiss-error",
                                "Close",
                                true,
                                cx,
                                |this, _, _| this.controller.error = None,
                            )),
                    ),
            );
        }
        if self.controller.error != self.attention_error {
            self.attention_error = self.controller.error.clone();
            if self.attention_error.is_some() {
                self.accessibility.focus_control("dismiss-error", window);
            }
        }
        body
    }
}
/// A full-width bar for notices above the canvas.
/// The layer an overlay draws in. It stays while the overlay is closed, so
/// the overlay's own exit plays when it closes rather than vanishing with
/// its container. Empty, it takes no input.
fn overlay_layer(key: &str, overlay: Option<AnyElement>) -> Div {
    div()
        .id(format!("layer-{key}"))
        .absolute()
        .inset_0()
        .children(overlay)
}
fn notice_bar() -> Div {
    div().px_4().py_2().flex().items_center().gap_3()
}
/// A notice bar with a message; actions can be added as further children.
fn notice_row(message: impl Into<SharedString>) -> Div {
    let message: SharedString = message.into();
    // Each notice slides down into the bar and folds away when it ends.
    notice_bar()
        .id(format!("notice-{message}"))
        .el(|e| {
            e.transition(
                rune::Transition::offset(0., -10.)
                    .combined(rune::Transition::opacity())
                    .hold_layout()
                    .springs(rune::SpringPair::new(crate::motion::SETTLE, crate::motion::SETTLE)),
            )
            .motion(crate::motion::SETTLE)
            .animated_changes()
        })
        .child(div().flex_1().text_sm().child(message))
}

#[cfg(test)]
mod window_title_tests {
    use super::window_title_for;

    #[::core::prelude::v1::test]
    fn window_title_tracks_library_and_active_note() {
        assert_eq!(window_title_for(true, "Lecture 1"), "Folio");
        assert_eq!(window_title_for(false, "Lecture 1"), "Lecture 1 — Folio");
        assert_eq!(
            window_title_for(false, "  Lecture 1  "),
            "Lecture 1 — Folio"
        );
        assert_eq!(window_title_for(false, "   "), "Folio");
        assert_eq!(window_title_for(false, ""), "Folio");
    }
}
