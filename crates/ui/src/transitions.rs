//! Enter and exit animations for menus, dialogs, sidebars and the tool
//! wheel, as Rune presence transitions.
use super::compat::*;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
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


#[derive(Default)]
pub(super) struct Transitions {
    styles: HashMap<&'static str, Style>,
    library: bool,
    switched: bool,
    /// Overlays built last frame and this one, and how many times each has
    /// opened: a menu's growth replays only under a new key.
    shown: HashSet<&'static str>,
    built: HashSet<&'static str>,
    opened: HashMap<&'static str, u64>,
    /// Where each morphing menu was last laid out, so it can shrink back
    /// into the point it grew from once it closes.
    morphs: HashMap<&'static str, Rc<Cell<Morph>>>,
    /// The selection toolbar springs to a new place (as it turns into a
    /// page and back) instead of following its selection at once.
    pub selection_glides: bool,
}

/// A morphing menu's last layout, in window points: its box, the box of
/// the holder around it, and the point it grows from in units of its box.
#[derive(Clone, Copy, Default)]
struct Morph {
    menu: [f32; 4],
    holder: [f32; 4],
    from: (f32, f32),
}
impl Morph {
    /// How far the point the menu grew from lies from the holder's corner.
    fn point(self) -> (f32, f32) {
        let [x, y, w, h] = self.menu;
        let (fx, fy) = self.from;
        (x + fx * w - self.holder[0], y + fy * h - self.holder[1])
    }
}

impl Transitions {
    /// Record whether the library is shown, returning whether that changed
    /// since the last frame.
    pub fn switch_view(&mut self, library: bool) -> bool {
        self.switched = std::mem::replace(&mut self.library, library) != library;
        self.switched
    }
    /// Whether this frame switched between the library and the editor.
    pub fn switched(&self) -> bool {
        self.switched
    }
    pub fn begin_frame(&mut self) {
        self.shown = std::mem::take(&mut self.built);
    }
    /// Record the style of the overlay `key`.
    pub fn observe(&mut self, key: &'static str, style: Style, _: Option<String>, _: bool) {
        self.styles.insert(key, style);
    }
    /// Draw `child` as the overlay `key`. Rune plays its entrance and, in
    /// reverse, its exit on springs: menus drop from their anchor, the tool
    /// wheel grows from its centre, sidebars slide from their edge and the
    /// selection toolbar pops up from the selection. (`_t` was the progress
    /// of the old per-frame tween; the view no longer redraws for it.)
    pub fn overlay(&mut self, key: &'static str, _t: f32, child: impl IntoElement) -> AnyElement {
        let style = self.styles.get(key).copied().unwrap_or(Style::Menu);
        let transition = overlay_transition(key, style);
        // The selection toolbar stays on its selection as it moves.
        let follows = key == "selection-toolbar";
        let morphs = style == Style::Menu && !follows;
        self.built.insert(key);
        let opened = self.opened.entry(key).or_default();
        if !self.shown.contains(key) {
            *opened += 1;
        }
        // Each opening is a new element, so its growth plays again.
        let n = *opened;
        let id = format!("overlay-{key}-{n}");
        let menu = match child.into_any_element() {
            AnyElement::Div(d) => *d,
            other => div().child(other),
        };
        let glides = follows && self.selection_glides;
        let motion = move |e: rune::El| {
            let e = e.transition(transition).animated_changes();
            let e = if morphs { e.sequence(rune::Sequence::new(MORPH)) } else { e };
            if glides {
                e.motion(crate::motion::MORPH)
            } else if follows {
                e.motion(rune::Spring::NONE)
            } else {
                e
            }
        };
        if !morphs {
            return menu.id(id).el(motion).into_any_element();
        }
        let morph = self.morphs.entry(key).or_default().clone();
        morph.set(Morph { from: menu.growth_point().unwrap_or((0.5, 0.5)), ..morph.get() });
        let seen = morph.clone();
        let menu = menu.id(id).el(motion).el(move |e| {
            e.on_layout_tx(move |_: &mut crate::AppState, menu: [f32; 4], tx: &mut rune::Tx| {
                seen.set(Morph { menu, ..seen.get() });
                tx.live_only();
            })
        });
        morph_holder(key, n, morph, Some(menu)).into_any_element()
    }
    /// What stays of the morphing menu `key` while it is closed: the holder
    /// it opened in, shrinking toward the point the menu grew from and
    /// taking the leaving menu with it. Put it where the menu was; `None`
    /// for overlays that don't morph or have not opened.
    pub fn closed(&self, key: &'static str) -> Option<AnyElement> {
        let morph = self.morphs.get(key)?;
        if self.built.contains(key) {
            return None;
        }
        let n = self.opened.get(key).copied()?;
        Some(morph_holder(key, n, morph.clone(), None).into_any_element())
    }
    /// Draw `child` as the box of the dialog `key`: it rises a little and
    /// settles at full size while its scrim fades in. A dialog replaced by
    /// another (a new `key`) swaps only the box; the scrim stays.
    ///
    /// The motion goes on the glass box itself: a blur inside a fading
    /// ancestor only sees that ancestor's own drawing, so it would show
    /// nothing until the fade ended and then snap in.
    pub fn dialog_box(key: &str, child: Div) -> Div {
        child
            .id(format!("{DIALOG_BOX}-{key}"))
            .el(|e| {
                e.transition(
                    rune::Transition::offset(0., 14.)
                        .combined(rune::Transition::scale(0.96))
                        .combined(rune::Transition::opacity())
                        .springs(rune::SpringPair::new(crate::motion::POP, crate::motion::SETTLE)),
                )
                .animated_changes()
            })
    }
    /// The container `panel` of a dialog's box. It must not fade in as a
    /// whole (new elements otherwise do): the box fades itself, so its blur
    /// shows from the first frame. Closing, it fades out beside its scrim.
    pub fn dialog_layer(panel: Div) -> Div {
        panel.el(|e| {
            e.transition(
                rune::Transition::asymmetric(rune::Transition::identity(), rune::Transition::opacity())
                    .springs(rune::SpringPair::new(crate::motion::SETTLE, crate::motion::SETTLE)),
            )
        })
    }
    /// The dialog `key` over its scrim, in a layer that stays mounted. A
    /// blur inside a fading ancestor only sees what that ancestor drew, so
    /// it would pop in when the fade ended and vanish when the dialog
    /// closed; the scrim instead comes and goes as a child of this layer,
    /// fading its blur with it.
    pub fn dialog_overlay(key: &str, theme: crate::theme::Theme, panel: Option<AnyElement>) -> Div {
        let open = panel.is_some();
        div()
            .id(format!("layer-{key}"))
            .absolute()
            .inset_0()
            .when(open, |layer| layer.child(Self::scrim(key, theme)))
            .children(panel)
    }
    /// The dimmed, blurred backdrop of the dialog `key`, beneath its box.
    /// It fades in and out on its own, so its blur fades with it.
    fn scrim(key: &str, theme: crate::theme::Theme) -> Div {
        use crate::theme::Glass;
        div()
            .id(format!("scrim-{key}"))
            .absolute()
            .inset_0()
            .scrim(theme)
            .el(|e| {
                e.transition(rune::Transition::opacity().springs(rune::SpringPair::new(
                    crate::motion::SETTLE,
                    crate::motion::SETTLE,
                )))
                .animated_changes()
            })
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

/// A menu opening: it grows out of a point at what opened it (its
/// `Div::grows_from`) into its full shape, overshooting a touch, without
/// fading. Transitions cannot start below half size; a sequence can.
static MORPH: &[rune::Beat] = &[
    rune::Beat::new(0.).scale(0.),
    rune::Beat::new(0.26)
        .scale(1.)
        .easing(rune::Easing::cubic(0.2, 1.18, 0.36, 1.)),
];

/// A menu closing: what it opened in shrinks to nothing about the point
/// the menu grew from, gathering speed as it goes, the opening reversed.
const SHRINK: rune::Curve = rune::Curve::tween(0.2, rune::Easing::cubic(0.4, 0., 0.8, 0.3));

/// The element the `n`th opening of the menu `key` sits in, covering what
/// the menu is positioned against. Once the menu has closed (`None`) it
/// shrinks to the point the menu grew from: a presence transition cannot
/// scale below half size, so the leaving menu, which can't change any more,
/// rides this instead. Its corner is the pivot and an offset on the same
/// curve carries the shrinking box onto the point, as the holder may have
/// no size (a menu anchored below a button). It shows and goes at once, so
/// nothing around a glass menu fades and its blur keeps sampling the window.
///
/// The menu sits in a second layer, which a menu placed beside its parent
/// (`top_full`) is anchored to: an anchored overlay takes the scale of its
/// anchor's ancestors, not its own.
fn morph_holder(key: &str, n: u64, morph: Rc<Cell<Morph>>, menu: Option<Div>) -> Div {
    let closing = menu.is_none();
    let (dx, dy) = morph.get().point();
    let inner = div()
        .id(format!("morph-{key}-{n}-in"))
        .absolute()
        .inset_0()
        .el(|e| e.transition(rune::Transition::none()))
        .children(menu);
    div().id(format!("morph-{key}-{n}")).absolute().inset_0().child(inner).el(move |e| {
        let e = e
            .transition(rune::Transition::none())
            .origin(0., 0.)
            .scale_motion(SHRINK)
            .offset_motion(SHRINK)
            .animated_changes()
            .on_layout_tx(move |_: &mut crate::AppState, holder: [f32; 4], tx: &mut rune::Tx| {
                morph.set(Morph { holder, ..morph.get() });
                tx.live_only();
            });
        if closing { e.scale(0.).offset(dx, dy) } else { e }
    })
}

/// How the overlay `key` of `style` enters and leaves.
fn overlay_transition(key: &str, style: Style) -> rune::Transition {
    use rune::{SpringPair, Transition, Vec2};
    use crate::motion::{POP, SETTLE};
    let fade = Transition::opacity();
    if key == "selection-toolbar" {
        return Transition::scale_anchor(0.85, Vec2::new(0.5, 1.0))
            .combined(Transition::offset(0., 6.))
            .combined(fade)
            .springs(SpringPair::new(POP, SETTLE));
    }
    match style {
        // Menus grow in on `MORPH` and shrink back into the point they
        // grew from with their holder (`morph_holder`). Rune fades every
        // leaving element; this one fades slowly, so it stays solid until
        // it is small.
        Style::Menu => Transition::identity().springs(SpringPair::new(POP, rune::Spring::new(0.7, 0.))),
        // The scrim and the box animate themselves (see `scrim` and
        // `dialog_box`); a fade here would hide their blur until it ended.
        Style::Dialog => Transition::identity().springs(SpringPair::new(SETTLE, SETTLE)),
        Style::Wheel => Transition::scale(0.8)
            .combined(fade)
            .allow_small_scale()
            .springs(SpringPair::new(POP, SETTLE)),
        Style::SlideLeft => Transition::offset(-28., 0.)
            .combined(fade)
            .springs(SpringPair::new(SETTLE, SETTLE)),
        Style::SlideRight => Transition::offset(28., 0.)
            .combined(fade)
            .springs(SpringPair::new(SETTLE, SETTLE)),
    }
}
