//! Building the window contents each frame.
use super::*;

/// Fade-in opacity of each animated panel for this frame.
struct PanelOpacity {
    library: f32,
    settings: f32,
    modal: f32,
    menu: f32,
    help: f32,
    popover: f32,
    tool_menu: f32,
    pages: f32,
}

impl Render for NotesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.release_hidden_resources(window, cx);
        self.controller.refresh_default_ink();
        self.apply_equation_result(window, cx);
        self.cancel_region_after_navigation();
        self.building_overlay = false;
        self.accessibility.begin();
        self.open_pending_dialogs(window, cx);
        let alpha = self.panel_opacity();
        self.track_recognition_frame(alpha.modal, window, cx);
        let theme = Theme::new(&self.controller.settings);
        window.set_rem_size(px(16. * self.controller.settings.ui_scale));
        // Build order matters: controls register for accessibility as they are built.
        let center = self.canvas_area(cx);
        let mut root = self.root_element(cx);
        let mut body = div().relative().flex().flex_1().min_h_0().min_w_0();
        if self.library_open {
            self.canvas_bounds = None;
            body = body.child(self.library_sidebar(cx)).child(
                self.library(window, cx)
                    .opacity(alpha.library)
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        for path in paths.paths() {
                            let id = this.controller.import_as_note(path.clone());
                            this.open_tabs.push(id);
                        }
                        this.show_editor();
                        cx.notify();
                    })),
            );
        } else {
            body = body.child(self.editor_column(center, alpha.pages, window, cx));
        }
        let canvas_overlay = self.canvas_menu_overlay(window, cx);
        body = self.add_overlays(body, &alpha, window, cx);
        self.building_overlay = false;
        root = root.child(self.document_tabs(window, cx));
        self.store_workspace();
        if self.controller.recognition_pending && !self.controller.recognition_setup_needed() {
            root = root.child(self.recognition_notice(cx));
        }
        if let Some(error) = self.controller.save_error.clone() {
            root = root.child(
                notice_row(format!("Changes are not saved. {error}"))
                    .bg(rgb(theme.surface))
                    .child(
                        self.button("retry-save", "Retry saving", false, cx, |this, _, _| {
                            this.controller.retry_save()
                        }),
                    ),
            );
        }
        root = root.child(body).children(canvas_overlay);
        if !self.library_open
            && !self.blocking_overlay()
            && (self.more_open || self.pen_settings || self.export_open)
        {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.popover(window, cx))
                    .opacity(alpha.popover),
            );
        }
        if self.tool_menu.is_some() && !self.library_open && !self.blocking_overlay() {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.tool_menu_panel(window, cx))
                    .opacity(alpha.tool_menu),
            );
        }
        if self.update_preparing {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(rgba(0x00000070))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .p_6()
                            .rounded(px(theme.radius))
                            .bg(rgb(theme.surface))
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
        self.recover_lost_focus(window, cx);
        self.resume_poll(cx);
        self.accessibility.publish(self, window, cx);
        titlebar::frame(root, window)
    }
}

impl NotesView {
    /// A control that disappears while focused (a menu item that closes its
    /// menu, a section that is replaced) leaves keyboard focus outside the
    /// window's key contexts, disabling every shortcut. Return focus to the
    /// root once the focused element has been missing for a whole frame;
    /// newly focused fields get that frame to render.
    fn recover_lost_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window
            .focused(cx)
            .is_some_and(|handle| handle != self.focus)
        {
            self.lost_focus = None;
            return;
        }
        // Check against the frame being built now, once it has been drawn.
        cx.on_next_frame(window, |this, window, cx| {
            let Some(handle) = window.focused(cx) else {
                return;
            };
            if handle == this.focus || this.focus.contains_focused(window, cx) {
                this.lost_focus = None;
            } else if this.lost_focus.as_ref() == Some(&handle) {
                this.lost_focus = None;
                this.focus.focus(window);
                cx.notify();
            } else {
                this.lost_focus = Some(handle);
                cx.notify();
            }
        });
    }
    /// Advance panel animations. Each panel animates when its key changes,
    /// so switching between two contents of one panel also fades it.
    fn panel_opacity(&self) -> PanelOpacity {
        let reduced = self.controller.settings.reduce_motion;
        let mut motion = self.motion.borrow_mut();
        motion.prune();
        if reduced {
            motion.settle();
        }
        let open = |shown: bool| shown.then(|| "open".to_string());
        PanelOpacity {
            library: motion.panel(
                "library-motion",
                self.library_open.then(|| {
                    format!(
                        "{:?}-{}-{}",
                        self.controller.filter, self.list_view, self.sort_by_name
                    )
                }),
                reduced,
            ),
            settings: motion.panel("settings-motion", open(self.settings_open), reduced),
            modal: motion.panel(
                "modal-motion",
                self.modal.as_ref().map(|(m, _)| m.title().into()),
                reduced,
            ),
            menu: motion.panel(
                "menu-motion",
                self.document_menu
                    .map(|(id, _)| id.to_string())
                    .or_else(|| self.folder_menu.map(|(id, _)| format!("folder-{id}"))),
                reduced,
            ),
            help: motion.panel("help-motion", open(self.help_open), reduced),
            popover: motion.panel(
                "popover-motion",
                (!self.library_open && (self.more_open || self.pen_settings || self.export_open))
                    .then(|| {
                        format!(
                            "{}-{}-{}",
                            self.more_open, self.pen_settings, self.export_open
                        )
                    }),
                reduced,
            ),
            tool_menu: motion.panel(
                "tool-menu-motion",
                self.tool_menu.map(|(kind, _)| format!("{kind:?}")),
                reduced,
            ),
            pages: motion.panel(
                "pages-motion",
                open(!self.library_open && self.pages_open),
                reduced,
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
        }
    }
    /// Close the equation dialog once its submitted LaTeX has rendered, or
    /// show the rendering error.
    fn apply_equation_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.equation_draft.is_some()
            && let Some(result) = self.controller.equation_result.take()
        {
            match result {
                Ok(()) => {
                    let submitted = self.equation_draft.take().unwrap();
                    if self
                        .modal
                        .as_ref()
                        .is_some_and(|(_, field)| field.read(cx).content == submitted)
                    {
                        self.close_modal(window, cx);
                    } else {
                        self.modal_error = Some(
                            "The submitted equation was saved. Your newer draft is still here."
                                .into(),
                        );
                    }
                }
                Err(error) => {
                    self.equation_draft = None;
                    self.modal_error = Some(error);
                    if let Some((_, field)) = &self.modal {
                        field.read(cx).focus.focus(window);
                    }
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
            self.modal(Modal::Recognition, window, cx);
        }
        if self.modal.is_none()
            && let Some((note, path)) = self.controller.pending_pdf_password.take()
        {
            self.modal(Modal::PdfPassword(note, path), window, cx);
        }
    }
    /// Report the first frame that shows a recognition result, for OCR timing.
    fn track_recognition_frame(
        &self,
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
            let view = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = view.update(cx, |view, _| {
                    view.controller.recognition_frame_completed(request)
                });
            });
        }
    }
    /// The document canvas: paints the page and routes pointer input.
    fn canvas_element(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        canvas(
            move |bounds, window, cx| {
                entity.update(cx, |view, cx| view.sync_canvas_bounds(bounds, cx));
                window.insert_hitbox(bounds, HitboxBehavior::Normal)
            },
            move |bounds, hitbox, window, cx| {
                paint_entity.update(cx, |view, cx| {
                    let editing = view.inline_text.as_ref().map(|e| e.id);
                    view.painter
                        .paint(&mut view.controller, bounds, editing, window, cx);
                    view.paint_region(bounds, window);
                    view.diagnostics.painted();
                    view.resume_poll(cx);
                    // The canvas paints its own pen cursor; elsewhere the pen
                    // shows the system cursor.
                    if view.controller.cursor.is_some() && !view.canvas_input_blocked() {
                        window.set_tablet_cursor_style(CursorStyle::None, &hitbox);
                    }
                });
                let hitbox = hitbox.id;
                let tablet_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &TabletEvent, phase, window, cx| {
                    if phase.bubble()
                        && (event.phase != TabletPhase::Down || hitbox.is_hovered(window))
                    {
                        tablet_entity.update(cx, |v, cx| v.tablet(event, window, cx));
                    }
                });
                let down_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase.bubble()
                        && hitbox.is_hovered(window)
                        && matches!(
                            event.button,
                            MouseButton::Left | MouseButton::Middle | MouseButton::Right
                        )
                    {
                        down_entity.update(cx, |v, cx| {
                            if event.button == MouseButton::Right {
                                v.open_canvas_menu(event.position, window, cx);
                            } else {
                                v.mouse(event.position, Phase::Down, event.button, window, cx);
                            }
                        });
                    }
                });
                let move_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase.bubble() {
                        move_entity.update(cx, |v, cx| {
                            let phase = if v.controller.interaction.is_some()
                                || (v.region_selection.is_some() && event.pressed_button.is_some())
                            {
                                Phase::Move
                            } else {
                                Phase::Hover
                            };
                            v.mouse(
                                event.position,
                                phase,
                                event.pressed_button.unwrap_or(MouseButton::Left),
                                window,
                                cx,
                            )
                        });
                    }
                });
                let up_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase.bubble() {
                        up_entity.update(cx, |v, cx| {
                            v.mouse(event.position, Phase::Up, event.button, window, cx)
                        });
                    }
                });
                let gesture_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &NavigationGesture, phase, _, cx| {
                    if phase.bubble() {
                        gesture_entity.update(cx, |v, cx| v.gesture(event, cx));
                    }
                });
                let pad_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &TabletPadEvent, phase, _, cx| {
                    if phase.bubble() {
                        pad_entity.update(cx, |v, cx| v.pad(event, cx));
                    }
                });
                let scroll_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase.bubble() && hitbox.should_handle_scroll(window) {
                        scroll_entity.update(cx, |v, cx| v.scroll(event, cx));
                    }
                });
            },
        )
        .size_full()
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
            self.controller.session_mut().viewport.pan.x +=
                f32::from(bounds.size.width - previous.size.width) / 2.;
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
        self.canvas_bounds = Some(bounds);
        self.canvas_document = Some(canvas_document);
    }
    /// The canvas with the overlays that sit on it: inline text editing, the
    /// loading notice and the selection toolbar.
    fn canvas_area(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let canvas = self.canvas_element(cx);
        let mut center = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(theme.selected))
            .child(canvas)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                let target = this.controller.requested_note();
                for path in paths.paths() {
                    this.controller.import_into(target, path.clone());
                }
                cx.notify();
            }));
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
        if !self.library_open
            && !self.controller.read_only()
            && !self.controller.session().selection.is_empty()
            && self.inline_text.is_none()
        {
            center = center.child(self.selection_toolbar(cx));
        }
        center
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
            .on_action(cx.listener(|_, _: &FocusNext, w, _| w.focus_next()))
            .on_action(cx.listener(|_, _: &FocusPrevious, w, _| w.focus_prev()))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| {
                // Same dialog as the New document button; Enter accepts the
                // defaults.
                this.modal(Modal::NewDocument, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewPage, _, cx| {
                this.controller.add_page();
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
            .on_action(cx.listener(|this, _: &Search, w, cx| this.modal(Modal::Search, w, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
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
            .on_action(cx.listener(|this, _: &Lasso, _, cx| {
                this.select_tool(Tool::Lasso);
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
                this.fit();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                let p = this.controller.session().page;
                this.controller.change_page(p.saturating_sub(1));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                let p = this.controller.session().page;
                this.controller.change_page(p + 1);
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
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.controller.error.is_some() && self.controller.interaction.is_none() {
            self.controller.error = None;
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
        } else if self.tool_menu.is_some() || self.sort_open {
            self.tool_menu = None;
            self.sort_open = false;
        } else if self.more_open || self.pen_settings || self.export_open {
            self.dismiss_popovers();
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
    fn editor_column(
        &mut self,
        center: Div,
        pages_alpha: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
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
            workspace = workspace.child(
                div()
                    .flex()
                    .h_full()
                    .child(self.pages_panel(cx))
                    .opacity(pages_alpha),
            );
        }
        workspace = workspace.child(center);
        if self.controller.math_session.is_some() {
            let floating =
                f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 1100.;
            workspace = workspace.child(self.math_panel(window, cx).when(floating, |panel| {
                panel.absolute().right_0().top_0().bottom_0().shadow_lg()
            }));
        }
        let region_notice = self.region_selection.is_some().then(|| {
            notice_bar()
                .child("Drag a rectangle on the page")
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
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .children(trash_notice)
            .children(import_notice)
            .children(stale_index_notice)
            .children(region_notice)
            .children(input_check)
            .children(formatting)
            .children(preview_notice)
            .child(workspace)
            .child(footer)
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
    fn canvas_menu_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        if self.canvas_menu.is_some() {
            self.building_overlay = true;
            self.accessibility.begin();
            Some(
                div()
                    .absolute()
                    .inset_0()
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
                    .child(self.canvas_menu_panel(window, cx)),
            )
        } else {
            None
        }
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
        if self.document_menu.is_some() {
            self.building_overlay = self.modal.is_none()
                && !self.settings_open
                && !self.help_open
                && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.document_menu_panel(window, cx))
                    .opacity(alpha.menu),
            );
        }
        if self.folder_menu.is_some() {
            self.building_overlay = self.modal.is_none()
                && !self.settings_open
                && !self.help_open
                && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.folder_menu_panel(window, cx))
                    .opacity(alpha.menu),
            );
        }
        if self.settings_open {
            self.building_overlay =
                self.modal.is_none() && !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.settings_panel(window, cx))
                    .opacity(alpha.settings),
            );
        }
        if self.modal.is_some() {
            self.building_overlay = !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.modal_panel(window, cx))
                    .opacity(alpha.modal),
            );
        }
        if self.help_open {
            self.building_overlay = self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.help_panel(window, cx))
                    .opacity(alpha.help),
            );
        }
        if self.controller.recognition_setup_needed() && self.controller.error.is_none() {
            self.building_overlay = true;
            self.accessibility.begin();
            body = body.child(self.ocr_setup_panel(cx));
            if !self.attention_setup {
                self.accessibility.focus_control("ocr-download", window);
            }
        }
        self.attention_setup = self.controller.recognition_setup_needed();
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
                    .bg(rgba(0x00000070))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(530.))
                            .p_6()
                            .bg(rgb(theme.surface))
                            .rounded(px(theme.radius + 4.))
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
fn notice_bar() -> Div {
    div().px_4().py_2().flex().items_center().gap_3()
}
/// A notice bar with a message; actions can be added as further children.
fn notice_row(message: impl Into<SharedString>) -> Div {
    notice_bar().child(div().flex_1().text_sm().child(message.into()))
}
