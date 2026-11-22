//! The Rune window: app state, the per-frame view build, background polling
//! and keyboard shortcut dispatch.
use super::*;
use std::cell::RefCell;

/// Rune's app state. Folio's views build with `&mut self` (they keep caches
/// and animation state), so the view sits in a cell the frame borrows.
pub struct AppState {
    pub view: RefCell<NotesView>,
    bindings: Vec<KeyBinding>,
    handle: Option<rune::AppHandle<AppState>>,
}

impl AppState {
    pub fn new(view: NotesView) -> Self {
        let mut bindings = NotesView::bindings();
        if view.titlebar_smoke {
            bindings.extend(NotesView::titlebar_smoke_bindings());
        }
        Self {
            view: RefCell::new(view),
            bindings,
            handle: None,
        }
    }
}

fn view_fn(s: &AppState, cx: &mut rune::Cx<AppState>) -> rune::El {
    let mut view = s.view.borrow_mut();
    view.system_dark = cx.system_dark();
    view.scale = cx.env().scale;
    let insets = cx.chrome_insets();
    view.chrome_trailing = insets.trailing;
    view.chrome_leading = insets.leading;
    if (view.controller.settings.ui_scale - 1.).abs() > 0.001 {
        cx.titlebar_height(40. * view.controller.settings.ui_scale);
    }
    let ui_scale = view.controller.settings.ui_scale;
    if (ui_scale - 1.).abs() > 0.001 {
        cx.zoom(ui_scale);
    }
    let theme = Theme::new(&view.controller.settings);
    cx.foreground(rgb(theme.ink).color());
    // Tooltips as Folio's hints drew them: ink fill, surface text.
    cx.help_look(
        rune::HelpLook::system(theme.dark)
            .fill(rgb(theme.ink).color())
            .ink(rgb(theme.surface).color())
            .radius(theme.radius)
            .padding(12., 8.)
            .font(rune::TextStyle::new(12., 400.).lh(19.))
            .shadows([
                rune::Shadow {
                    x: 0.,
                    y: 4.,
                    blur: 6.,
                    spread: -1.,
                    color: rgba(0x0000001a).color(),
                    inset: false,
                },
                rune::Shadow {
                    x: 0.,
                    y: 2.,
                    blur: 4.,
                    spread: -2.,
                    color: rgba(0x0000001a).color(),
                    inset: false,
                },
            ]),
    );
    if view.controller.settings.reduce_motion {
        cx.reduced_motion(true);
    }
    compat::clear_actions();
    paint::begin_frame(view.scale);
    compat::begin_drops();
    compat::begin_anchors();
    let started = std::time::Instant::now();
    let mut window = Window::render(&view);
    let mut context = Context::fresh();
    let element = div()
        .size_full()
        .child(view.render(&mut window, &mut context))
        .children(compat::drag_preview())
        .into_any_element();
    let root_text = TextProps {
        size: Some(REM),
        color: Some(rgb(theme.ink).color()),
        ..Default::default()
    };
    let built = {
        let mut bcx = BuildCx {
            view: &mut view,
            window: &mut window,
            cx: &mut context,
        };
        element.build(&root_text, &mut bcx)
    };
    view.deferred.extend(window.effects);
    view.deferred.extend(context.effects);
    if paint::profiling() {
        eprintln!(
            "FOLIO_VIEW build {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.
        );
    }
    built
        .instant_changes()
        .on_layout(|s: &mut AppState, r: [f32; 4]| {
            let view = s.view.get_mut();
            let size = size(px(r[2]), px(r[3]));
            if view.viewport != size {
                view.viewport = size;
            }
        })
        .on_pointer_move_local(
            |s: &mut AppState, p: rune::PointerEvent, tx: &mut rune::Tx| {
                let view = s.view.get_mut();
                view.mouse_position = point(px(p.window_x), px(p.window_y));
                view.window_hovered = true;
                if !view.pointer_tracking() {
                    tx.live_only();
                }
            },
        )
        .on_window_focus(|s: &mut AppState, focused: bool, _tx: &mut rune::Tx| {
            s.view.get_mut().window_hovered = focused;
        })
        .on_drop_files(
            |s: &mut AppState, files: &[std::path::PathBuf], tx: &mut rune::Tx| {
                let files = files.to_vec();
                dispatch(s, tx, |view, window, cx| view.drop_files(files, window, cx));
            },
        )
}

fn on_key(s: &mut AppState, key: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    let Some(event) = KeyDownEvent::from_rune(key) else {
        return false;
    };
    let chord = event.chord();
    let contexts = {
        let view = s.view.get_mut();
        view.focused_key = tx.focused().map(str::to_owned);
        view.key_contexts(tx.focused())
    };
    let contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
    let action = s
        .bindings
        .iter()
        .rev()
        .find(|b| b.keys == chord && b.matches_context(&contexts))
        .map(|b| b.action);
    if let Some(handler) = action.and_then(compat::action_handler) {
        dispatch(s, tx, |view, window, cx| handler(view, window, cx));
        return true;
    }
    // Keys no binding takes, such as the tool wheel's W release, go to the
    // view's own key handler.
    dispatch(s, tx, |view, window, cx| view.key_down(&event, window, cx))
}

fn poll(s: &mut AppState, _clock: f64, tx: &mut rune::Tx) -> bool {
    let reports = tablet::drain();
    let pen = !reports.is_empty();
    let scale = s.view.get_mut().scale;
    for report in reports {
        match report {
            winit::platform::tablet::TabletEvent::Tool(tool) => {
                let event = tablet::tool_event(&tool, scale);
                dispatch(s, tx, |view, window, cx| view.tablet(&event, window, cx));
            }
            winit::platform::tablet::TabletEvent::Pad(pad) => {
                let event = tablet::pad_event(&pad);
                dispatch(s, tx, |view, window, cx| view.pad(&event, window, cx));
            }
        }
    }
    let view = s.view.get_mut();
    view.focused_key = tx.focused().map(str::to_owned);
    view.poll(tx) || pen
}

fn no_init() -> AppState {
    unreachable!("Folio builds its state in AppState::init")
}

fn setup(s: &mut AppState, waker: rune::Waker) {
    executor::set_waker(waker.clone());
    tablet::install(waker.clone());
    let view = s.view.get_mut();
    view.controller.set_wakeup(waker.clone());
    view.updater.set_wakeup(waker);
}

/// Closing hides the window first (the app keeps running); Folio then quits,
/// or reopens the window to ask about unsaved changes.
fn on_close(s: &mut AppState, tx: &mut rune::Tx) {
    if dispatch(s, tx, |view, window, cx| view.should_close(window, cx)) {
        tx.quit();
    } else if let Some(handle) = &s.handle {
        handle.open();
    }
}

/// Opens Folio's window and runs until it closes. `ready` receives the app
/// handle before the event loop starts, for activation from later launches.
pub fn run(
    view: NotesView,
    open_editor: bool,
    rune_args: Vec<String>,
    ready: impl FnOnce(rune::AppHandle<AppState>),
) {
    let mut view = view;
    if open_editor {
        view.show_editor();
    }
    let spec = rune::AppSpec::<AppState> {
        poll: Some(poll),
        setup: Some(setup),
        on_key: Some(on_key),
        min_size: Some((1000., 620.)),
        families: (FONT_FAMILY_LIST, "monospace"),
        ..rune::AppSpec::new("Folio", (1320., 860.), no_init, view_fn)
    };
    // Folio draws its own titlebar band (the tab strip); Rune keeps the
    // window's controls over it on Windows and macOS. Linux keeps the
    // system frame, which resizes and decorates the window.
    let app = rune::App::new(spec).titlebar(rune::Titlebar::new(40.));
    let handle = app.handle();
    ready(handle.clone());
    app.args(rune_args)
        .init(move || {
            let mut app = AppState::new(view);
            app.handle = Some(handle);
            app
        })
        .keep_running()
        .on_close(on_close)
        .run();
}

/// Families the interface asks for, in order of preference.
pub const FONT_FAMILY_LIST: &str = "Noto Sans, DejaVu Sans, sans-serif";

impl NotesView {
    /// Whether the view draws something that follows the pointer, so
    /// pointer moves must rebuild it.
    pub(crate) fn pointer_tracking(&self) -> bool {
        self.tool_wheel.is_some() || self.tab_drag.is_some() || self.color_drag.is_some()
    }
    /// The key contexts GPUI's bindings were scoped to, from what is open and
    /// what has keyboard focus.
    pub(crate) fn key_contexts(&self, focused: Option<&str>) -> Vec<String> {
        let mut contexts: Vec<String> = if self.update_preparing {
            vec!["FolioUpdating".into()]
        } else if self.canvas_menu.is_some() {
            vec!["FolioCanvasMenu".into()]
        } else if self.blocking_overlay() || self.controller.loading_note() {
            vec!["FolioDialog".into()]
        } else if self.library_open {
            vec!["FolioLibrary".into()]
        } else if self.cropping() {
            vec!["Folio".into(), "FolioCrop".into()]
        } else {
            vec!["Folio".into()]
        };
        if let Some(key) = focused {
            if key.starts_with("field-") {
                contexts.push("FolioField".into());
                if self.find.is_some() {
                    contexts.push("FolioFind".into());
                }
            } else if key != self.focus.0 && key != "canvas-input" {
                contexts.push("FolioControl".into());
            }
        }
        contexts
    }
    /// Keys no binding took.
    pub(crate) fn key_down(
        &mut self,
        _event: &KeyDownEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }
    /// Whether the window may close now: edits are saved, or the person is
    /// asked first when autosave is off.
    pub(crate) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.update_preparing {
            return false;
        }
        if self.closing {
            return true;
        }
        self.commit_inline_text(cx);
        if self.needs_close_prompt() {
            self.modal(Modal::UnsavedChanges, window, cx);
            return false;
        }
        self.store_workspace();
        match self.controller.flush() {
            Ok(()) => true,
            Err(e) => {
                self.controller.error = Some(e);
                false
            }
        }
    }
}
