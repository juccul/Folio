//! Enter and exit animations for menus, dialogs, sidebars and the tool wheel.
//!
//! Each overlay paints through [`gpui::effect`], which applies its entrance
//! (fade, slide and slight scale) and keeps a snapshot of its last frame.
//! When an overlay closes, or is replaced by different content, that
//! snapshot is painted again for a moment while it fades away. Exits never
//! delay state changes or receive input.
use gpui::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

/// How an overlay enters and leaves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Style {
    /// A menu or popover that drops from its anchor.
    Menu,
    /// A dialog over a scrim. Its box (see [`Transitions::dialog_box`])
    /// rises and settles while the scrim fades.
    Dialog,
    /// The tool wheel, which grows from its center.
    Wheel,
    /// A sidebar docked to the left edge.
    SlideLeft,
    /// A sidebar docked to the right edge.
    SlideRight,
}

/// The tag of a dialog's box inside its overlay snapshot.
const DIALOG_BOX: &str = "dialog-box";

impl Style {
    fn exit_duration(self) -> Duration {
        Duration::from_millis(match self {
            Style::Menu => 130,
            Style::Wheel => 140,
            Style::Dialog => 170,
            Style::SlideLeft | Style::SlideRight => 180,
        })
    }

    /// The overlay's effect `t` (eased, 0 to 1) of the way through entering.
    pub(super) fn enter(self, t: f32) -> PaintEffect {
        let t = t.clamp(0., 1.);
        let rest = 1. - t;
        match self {
            Style::Menu => PaintEffect {
                opacity: t,
                offset: point(px(0.), px(-6. * rest)),
                scale: 0.96 + 0.04 * t,
                origin: point(0.5, 0.),
            },
            // The scrim only fades; the box moves.
            Style::Dialog => PaintEffect {
                opacity: t,
                ..PaintEffect::NONE
            },
            Style::Wheel => PaintEffect {
                opacity: t,
                scale: 0.86 + 0.14 * t,
                ..PaintEffect::NONE
            },
            Style::SlideLeft => PaintEffect {
                opacity: t,
                offset: point(px(-24. * rest), px(0.)),
                ..PaintEffect::NONE
            },
            Style::SlideRight => PaintEffect {
                opacity: t,
                offset: point(px(32. * rest), px(0.)),
                ..PaintEffect::NONE
            },
        }
    }

    /// Where the overlay is headed as it leaves.
    fn exit(self) -> PaintEffect {
        match self {
            Style::Menu => PaintEffect {
                opacity: 0.,
                offset: point(px(0.), px(-4.)),
                scale: 0.97,
                origin: point(0.5, 0.),
            },
            Style::Dialog => PaintEffect {
                opacity: 0.,
                ..PaintEffect::NONE
            },
            Style::Wheel => PaintEffect {
                opacity: 0.,
                scale: 0.9,
                ..PaintEffect::NONE
            },
            Style::SlideLeft => PaintEffect {
                opacity: 0.,
                offset: point(px(-24.), px(0.)),
                ..PaintEffect::NONE
            },
            Style::SlideRight => PaintEffect {
                opacity: 0.,
                offset: point(px(32.), px(0.)),
                ..PaintEffect::NONE
            },
        }
    }
}

/// A dialog box's effect `t` of the way through entering: it rises a little
/// and grows to full size.
fn dialog_box_enter(t: f32) -> PaintEffect {
    let t = t.clamp(0., 1.);
    PaintEffect {
        // Under the fading scrim this starts the box slightly after it.
        opacity: t,
        offset: point(px(0.), px(14. * (1. - t))),
        scale: 0.96 + 0.04 * t,
        ..PaintEffect::NONE
    }
}

/// Where a dialog box is headed as it leaves: down a little and smaller.
fn dialog_box_exit() -> PaintEffect {
    PaintEffect {
        offset: point(px(0.), px(8.)),
        scale: 0.97,
        ..PaintEffect::NONE
    }
}

struct Overlay {
    style: Style,
    signature: Option<String>,
    snapshot: Rc<RefCell<Option<PaintSnapshot>>>,
    /// The frame that last painted the overlay.
    painted: u64,
}

struct Exit {
    style: Style,
    snapshot: PaintSnapshot,
    /// Only the dialog box leaves; a new dialog keeps the scrim.
    box_only: bool,
    started: Instant,
}

#[derive(Default)]
pub(super) struct Transitions {
    overlays: HashMap<&'static str, Overlay>,
    exits: Vec<Exit>,
    frame: u64,
    library: bool,
}

impl Transitions {
    /// Record whether the library is shown, returning whether that changed
    /// since the last frame.
    pub fn switch_view(&mut self, library: bool) -> bool {
        std::mem::replace(&mut self.library, library) != library
    }

    /// Start a frame, forgetting exits that have finished.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let now = Instant::now();
        self.exits
            .retain(|exit| now.duration_since(exit.started) < exit.style.exit_duration());
    }

    /// Record which content the overlay `key` shows this frame (`None` when
    /// closed). Closing it, or replacing its content, starts an exit from
    /// the last frame that painted it.
    pub fn observe(
        &mut self,
        key: &'static str,
        style: Style,
        signature: Option<String>,
        animate: bool,
    ) {
        let frame = self.frame;
        let overlay = self.overlays.entry(key).or_insert_with(|| Overlay {
            style,
            signature: None,
            snapshot: Rc::default(),
            painted: 0,
        });
        overlay.style = style;
        if overlay.signature == signature {
            return;
        }
        let previous = std::mem::replace(&mut overlay.signature, signature);
        let snapshot = overlay.snapshot.borrow_mut().take();
        // Only a snapshot from the previous frame shows what was on screen.
        if previous.is_some()
            && animate
            && overlay.painted + 1 == frame
            && let Some(snapshot) = snapshot
        {
            let box_only = style == Style::Dialog && overlay.signature.is_some();
            let snapshot = if box_only {
                snapshot.find(DIALOG_BOX)
            } else {
                Some(snapshot)
            };
            if let Some(snapshot) = snapshot {
                self.exits.push(Exit {
                    style,
                    snapshot,
                    box_only,
                    started: Instant::now(),
                });
            }
        }
    }

    /// Paint `child` as the overlay `key`, `t` (eased) of the way through
    /// entering.
    pub fn overlay(&mut self, key: &'static str, t: f32, child: impl IntoElement) -> Effected {
        let frame = self.frame;
        let overlay = self.overlays.entry(key).or_insert_with(|| Overlay {
            style: Style::Menu,
            signature: None,
            snapshot: Rc::default(),
            painted: 0,
        });
        overlay.painted = frame;
        effect(
            key,
            overlay.style.enter(t),
            Some(overlay.snapshot.clone()),
            child,
        )
    }

    /// Paint `child` as the box of a dialog overlay, `t` of the way through
    /// entering.
    pub fn dialog_box(t: f32, child: impl IntoElement) -> Effected {
        effect(DIALOG_BOX, dialog_box_enter(t), None, child)
    }

    /// Whether an exit is still running.
    pub fn animating(&self) -> bool {
        !self.exits.is_empty()
    }

    /// The overlays that are leaving, to paint above the window contents.
    pub fn exits(&self) -> Vec<AnyElement> {
        let now = Instant::now();
        self.exits
            .iter()
            .map(|exit| {
                let progress = (now.duration_since(exit.started).as_secs_f32()
                    / exit.style.exit_duration().as_secs_f32())
                .clamp(0., 1.);
                // Ease out: the overlay starts leaving at once and slows as
                // it disappears.
                let eased = 1. - (1. - progress).powi(2);
                let style = exit.style;
                let box_only = exit.box_only;
                snapshot(exit.snapshot.clone(), move |node| {
                    let from = node.effect();
                    if node.tag().as_ref() == DIALOG_BOX {
                        let mut to = dialog_box_exit();
                        to.opacity = if box_only { 0. } else { from.opacity };
                        from.lerp(to, eased)
                    } else if !box_only && is_root(node) {
                        from.lerp(style.exit(), eased)
                    } else {
                        from
                    }
                })
                .absolute()
                .inset_0()
                .into_any_element()
            })
            .collect()
    }
}

/// Whether `node` is an overlay root rather than an element nested in it.
fn is_root(node: &PaintSnapshot) -> bool {
    OVERLAY_KEYS.contains(&node.tag().as_ref())
}

/// Every overlay key, so exits can tell overlays from their contents.
pub(super) const OVERLAY_KEYS: &[&str] = &[
    "popover",
    "tool-menu",
    "tool-wheel",
    "document-menu",
    "folder-menu",
    "canvas-menu",
    "library-sort-menu",
    "font-menu",
    "selection-toolbar",
    "settings",
    "modal",
    "help",
    "pages",
    "math",
];
