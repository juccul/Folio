//! Enter animations for menus, dialogs, sidebars and the tool wheel.
//!
//! Each overlay draws with its entrance effect (fade, slide and slight
//! scale) applied to its element. Rune's presence transitions fade an
//! overlay out when it closes.
use super::compat::*;
use std::{collections::HashMap, time::Duration};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct PaintEffect {
    pub opacity: f32,
    pub offset: Point<Pixels>,
    pub scale: f32,
    pub origin: Point<f32>,
}
impl PaintEffect {
    pub const NONE: PaintEffect = PaintEffect {
        opacity: 1.,
        offset: Point { x: Pixels(0.), y: Pixels(0.) },
        scale: 1.,
        origin: Point { x: 0.5, y: 0.5 },
    };
    fn apply(self, e: rune::El) -> rune::El {
        let mut e = e.opacity(self.opacity);
        if self.offset != Point::default() {
            e = e.offset(self.offset.x.0, self.offset.y.0);
        }
        if self.scale != 1. {
            e = e.scale(self.scale).origin(self.origin.x, self.origin.y);
        }
        e
    }
}


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

    }

/// A dialog box's effect `t` of the way through entering: it rises a little
/// and grows to full size.
fn dialog_box_enter(t: f32) -> PaintEffect {
    let t = t.clamp(0., 1.);
    PaintEffect {
        opacity: t,
        offset: point(px(0.), px(14. * (1. - t))),
        scale: 0.96 + 0.04 * t,
        ..PaintEffect::NONE
    }
}

#[derive(Default)]
pub(super) struct Transitions {
    styles: HashMap<&'static str, Style>,
    library: bool,
}

impl Transitions {
    /// Record whether the library is shown, returning whether that changed
    /// since the last frame.
    pub fn switch_view(&mut self, library: bool) -> bool {
        std::mem::replace(&mut self.library, library) != library
    }
    pub fn begin_frame(&mut self) {}
    /// Record the style of the overlay `key`.
    pub fn observe(&mut self, key: &'static str, style: Style, _: Option<String>, _: bool) {
        self.styles.insert(key, style);
    }
    /// Draw `child` as the overlay `key`, `t` (eased) of the way through entering.
    pub fn overlay(&mut self, key: &'static str, t: f32, child: impl IntoElement) -> AnyElement {
        let style = self.styles.get(key).copied().unwrap_or(Style::Menu);
        let effect = style.enter(t);
        div()
            .id(format!("overlay-{key}"))
            .absolute()
            .inset_0()
            .child(child)
            .el(move |e| effect.apply(e).transition(rune::Transition::opacity()))
            .into_any_element()
    }
    /// Draw `child` as the box of a dialog overlay, `t` of the way through entering.
    pub fn dialog_box(t: f32, child: impl IntoElement) -> Div {
        let effect = dialog_box_enter(t);
        div()
            .id(DIALOG_BOX)
            .flex()
            .flex_col()
            .items_center()
            .child(child)
            .el(move |e| effect.apply(e))
    }
    pub fn animating(&self) -> bool {
        false
    }
    pub fn exits(&self) -> Vec<AnyElement> {
        Vec::new()
    }
}
#[allow(dead_code)]
fn unused(_: Duration) {}
