//! Fades, offsets and scales applied to everything an element paints, and
//! retained snapshots of what it painted, so an element can animate out
//! after it has been removed.
use crate::{
    AnyElement, App, Background, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, PaintOperation, Pixels, Point, Primitive,
    ScaledPixels, SharedString, TransformationMatrix, Window, point, px,
};
use std::{cell::RefCell, panic, rc::Rc, sync::Arc};

/// A fade, offset and scale applied to an element's painted output. Layout
/// and hit testing are unchanged, so effects suit short transitions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintEffect {
    /// Multiplies the opacity of everything painted.
    pub opacity: f32,
    /// Moves the painted output.
    pub offset: Point<Pixels>,
    /// Scales the painted output around `origin`.
    pub scale: f32,
    /// The point scaling is centered on, as a fraction of the element's
    /// bounds: (0.5, 0.5) is its center.
    pub origin: Point<f32>,
}

impl Default for PaintEffect {
    fn default() -> Self {
        Self::NONE
    }
}

impl PaintEffect {
    /// No change.
    pub const NONE: Self = Self {
        opacity: 1.,
        offset: Point {
            x: Pixels(0.),
            y: Pixels(0.),
        },
        scale: 1.,
        origin: Point { x: 0.5, y: 0.5 },
    };

    /// Whether painting with this effect changes nothing.
    pub fn is_none(&self) -> bool {
        self.opacity == 1. && self.offset == Point::default() && self.scale == 1.
    }

    /// The effect `t` of the way from `self` to `other`. The origin is taken
    /// from `other`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            opacity: mix(self.opacity, other.opacity),
            offset: point(
                px(mix(self.offset.x.0, other.offset.x.0)),
                px(mix(self.offset.y.0, other.offset.y.0)),
            ),
            scale: mix(self.scale, other.scale),
            origin: other.origin,
        }
    }

    fn affine(self, bounds: Bounds<Pixels>, scale_factor: f32) -> Affine {
        let pivot = point(
            (bounds.origin.x.0 + bounds.size.width.0 * self.origin.x) * scale_factor,
            (bounds.origin.y.0 + bounds.size.height.0 * self.origin.y) * scale_factor,
        );
        Affine {
            scale: self.scale,
            translate: point(
                pivot.x * (1. - self.scale) + self.offset.x.0 * scale_factor,
                pivot.y * (1. - self.scale) + self.offset.y.0 * scale_factor,
            ),
            opacity: self.opacity.clamp(0., 1.),
        }
    }
}

/// `p * scale + translate` in device pixels, with an opacity multiplier.
#[derive(Clone, Copy)]
struct Affine {
    scale: f32,
    translate: Point<f32>,
    opacity: f32,
}

impl Affine {
    const IDENTITY: Self = Self {
        scale: 1.,
        translate: Point { x: 0., y: 0. },
        opacity: 1.,
    };

    /// `self` applied after `inner`.
    fn then(self, inner: Self) -> Self {
        Self {
            scale: self.scale * inner.scale,
            translate: point(
                self.scale * inner.translate.x + self.translate.x,
                self.scale * inner.translate.y + self.translate.y,
            ),
            opacity: self.opacity * inner.opacity,
        }
    }

    fn point(self, p: Point<ScaledPixels>) -> Point<ScaledPixels> {
        point(
            ScaledPixels(p.x.0 * self.scale + self.translate.x),
            ScaledPixels(p.y.0 * self.scale + self.translate.y),
        )
    }

    fn length(self, length: ScaledPixels) -> ScaledPixels {
        ScaledPixels(length.0 * self.scale)
    }

    fn bounds(self, bounds: Bounds<ScaledPixels>) -> Bounds<ScaledPixels> {
        Bounds {
            origin: self.point(bounds.origin),
            size: bounds.size.map(|length| self.length(length)),
        }
    }

    fn mask(self, mask: &mut ContentMask<ScaledPixels>) {
        mask.bounds = self.bounds(mask.bounds);
    }

    fn background(self, background: &mut Background) {
        background.solid.a *= self.opacity;
        for stop in &mut background.colors {
            stop.color.a *= self.opacity;
        }
    }

    fn matrix(self, matrix: TransformationMatrix) -> TransformationMatrix {
        TransformationMatrix {
            rotation_scale: [[self.scale, 0.], [0., self.scale]],
            translation: [self.translate.x, self.translate.y],
        }
        .compose(matrix)
    }

    fn apply(self, primitive: &mut Primitive) {
        match primitive {
            Primitive::BackdropBlur(blur) => {
                blur.bounds = self.bounds(blur.bounds);
                self.mask(&mut blur.content_mask);
                blur.corner_radii = blur.corner_radii.map(|r| self.length(*r));
                blur.opacity *= self.opacity;
            }
            Primitive::Shadow(shadow) => {
                shadow.bounds = self.bounds(shadow.bounds);
                shadow.cutout = self.bounds(shadow.cutout);
                self.mask(&mut shadow.content_mask);
                shadow.corner_radii = shadow.corner_radii.map(|r| self.length(*r));
                shadow.blur_radius = self.length(shadow.blur_radius);
                shadow.color.a *= self.opacity;
            }
            Primitive::Quad(quad) => {
                quad.bounds = self.bounds(quad.bounds);
                self.mask(&mut quad.content_mask);
                quad.corner_radii = quad.corner_radii.map(|r| self.length(*r));
                quad.border_widths = quad.border_widths.map(|w| self.length(*w));
                self.background(&mut quad.background);
                quad.border_color.a *= self.opacity;
            }
            Primitive::Path(path) => {
                path.bounds = self.bounds(path.bounds);
                self.mask(&mut path.content_mask);
                for vertex in &mut path.vertices {
                    vertex.xy_position = self.point(vertex.xy_position);
                    self.mask(&mut vertex.content_mask);
                }
                self.background(&mut path.color);
            }
            Primitive::Underline(underline) => {
                underline.bounds = self.bounds(underline.bounds);
                self.mask(&mut underline.content_mask);
                underline.thickness = self.length(underline.thickness);
                underline.color.a *= self.opacity;
            }
            Primitive::MonochromeSprite(sprite) => {
                // Sprites are positioned by their transformation, which is
                // applied to their untransformed bounds.
                sprite.transformation = self.matrix(sprite.transformation);
                self.mask(&mut sprite.content_mask);
                sprite.color.a *= self.opacity;
            }
            Primitive::PolychromeSprite(sprite) => {
                sprite.transformation = self.matrix(sprite.transformation);
                sprite.paint_bounds = self.bounds(sprite.paint_bounds);
                self.mask(&mut sprite.content_mask);
                sprite.opacity *= self.opacity;
            }
            Primitive::Surface(surface) => {
                surface.bounds = self.bounds(surface.bounds);
                self.mask(&mut surface.content_mask);
            }
        }
    }
}

/// What one element painted in a frame, including the effects of nested
/// [`effect`] elements, kept so it can be painted again later.
#[derive(Clone)]
pub struct PaintSnapshot(Arc<SnapshotNode>);

struct SnapshotNode {
    tag: SharedString,
    bounds: Bounds<Pixels>,
    effect: PaintEffect,
    operations: Vec<SnapshotOperation>,
}

enum SnapshotOperation {
    Paint(PaintOperation),
    Child(PaintSnapshot),
}

impl PaintSnapshot {
    /// The tag given to the [`effect`] element that painted this snapshot.
    pub fn tag(&self) -> &SharedString {
        &self.0.tag
    }

    /// The effect the element was painted with.
    pub fn effect(&self) -> PaintEffect {
        self.0.effect
    }

    /// The element's layout bounds.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.0.bounds
    }

    /// The first nested snapshot with `tag`, searching depth first.
    pub fn find(&self, tag: &str) -> Option<PaintSnapshot> {
        self.0
            .operations
            .iter()
            .find_map(|operation| match operation {
                SnapshotOperation::Child(child) if child.tag() == tag => Some(child.clone()),
                SnapshotOperation::Child(child) => child.find(tag),
                SnapshotOperation::Paint(_) => None,
            })
    }
}

/// An element being captured while it paints.
pub(crate) struct Capture {
    mark: crate::scene::SceneMark,
    /// Ranges of the scene's paint operations painted by nested effects.
    children: Vec<(usize, usize, PaintSnapshot)>,
}

impl Window {
    /// Paint with `effect` applied to everything `f` paints, returning a
    /// snapshot of it that [`Window::paint_snapshot`] can paint again.
    /// `bounds` are the element's bounds, used for the scale origin.
    pub fn paint_with_effect(
        &mut self,
        tag: SharedString,
        bounds: Bounds<Pixels>,
        effect: PaintEffect,
        f: impl FnOnce(&mut Self),
    ) -> PaintSnapshot {
        let mark = self.next_frame.scene.mark();
        let start = mark.operations;
        self.captures.push(Capture {
            mark,
            children: Vec::new(),
        });
        f(self);
        let capture = self.captures.pop().expect("unbalanced paint captures");
        let scale_factor = self.scale_factor();
        let scene = &mut self.next_frame.scene;
        let mut operations = Vec::new();
        let mut cursor = start;
        for (child_start, child_end, child) in capture.children {
            operations.extend(
                scene.paint_operations[cursor..child_start]
                    .iter()
                    .cloned()
                    .map(SnapshotOperation::Paint),
            );
            operations.push(SnapshotOperation::Child(child));
            cursor = child_end;
        }
        operations.extend(
            scene.paint_operations[cursor..]
                .iter()
                .cloned()
                .map(SnapshotOperation::Paint),
        );
        if !effect.is_none() {
            let painted = scene.take_since(&capture.mark);
            let affine = effect.affine(bounds, scale_factor);
            for operation in painted {
                insert(scene, operation, affine);
            }
        }
        let end = self.next_frame.scene.paint_operations.len();
        let snapshot = PaintSnapshot(Arc::new(SnapshotNode {
            tag,
            bounds,
            effect,
            operations,
        }));
        if let Some(parent) = self.captures.last_mut() {
            parent.children.push((start, end, snapshot.clone()));
        }
        snapshot
    }

    /// Paint a snapshot again, choosing each element's effect with
    /// `effect`. Images released since the snapshot are left out.
    pub fn paint_snapshot(
        &mut self,
        snapshot: &PaintSnapshot,
        effect: &dyn Fn(&PaintSnapshot) -> PaintEffect,
    ) {
        self.replay_snapshot(snapshot, Affine::IDENTITY, effect);
    }

    fn replay_snapshot(
        &mut self,
        snapshot: &PaintSnapshot,
        outer: Affine,
        effect: &dyn Fn(&PaintSnapshot) -> PaintEffect,
    ) {
        let affine = outer.then(effect(snapshot).affine(snapshot.bounds(), self.scale_factor()));
        for operation in &snapshot.0.operations {
            match operation {
                SnapshotOperation::Child(child) => self.replay_snapshot(child, affine, effect),
                SnapshotOperation::Paint(operation) => {
                    let tile = match operation {
                        PaintOperation::Primitive(Primitive::MonochromeSprite(sprite)) => {
                            Some(sprite.tile.texture_id)
                        }
                        PaintOperation::Primitive(Primitive::PolychromeSprite(sprite)) => {
                            Some(sprite.tile.texture_id)
                        }
                        _ => None,
                    };
                    if tile.is_some_and(|id| !self.sprite_atlas.contains_texture(id)) {
                        continue;
                    }
                    insert(&mut self.next_frame.scene, operation.clone(), affine);
                }
            }
        }
    }
}

fn insert(scene: &mut crate::Scene, operation: PaintOperation, affine: Affine) {
    match operation {
        PaintOperation::Primitive(mut primitive) => {
            affine.apply(&mut primitive);
            scene.insert_primitive(primitive);
        }
        PaintOperation::StartLayer(bounds) => scene.push_layer(affine.bounds(bounds)),
        PaintOperation::EndLayer => scene.pop_layer(),
    }
}

/// Wrap `child` so its painted output takes `effect`. When `sink` is given,
/// it receives a snapshot of each paint.
pub fn effect(
    tag: impl Into<SharedString>,
    effect: PaintEffect,
    sink: Option<Rc<RefCell<Option<PaintSnapshot>>>>,
    child: impl IntoElement,
) -> Effected {
    Effected {
        tag: tag.into(),
        effect,
        sink,
        child: Some(child.into_any_element()),
    }
}

/// An element whose painted output takes a [`PaintEffect`]. See [`effect`].
pub struct Effected {
    tag: SharedString,
    effect: PaintEffect,
    sink: Option<Rc<RefCell<Option<PaintSnapshot>>>>,
    child: Option<AnyElement>,
}

impl IntoElement for Effected {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Effected {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let child = self.child.as_mut().expect("effect child");
        (child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = &mut self.child {
            child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let snapshot = window.paint_with_effect(self.tag.clone(), bounds, self.effect, |window| {
            child.paint(window, cx)
        });
        if let Some(sink) = &self.sink {
            *sink.borrow_mut() = Some(snapshot);
        }
    }
}

/// Replays a [`PaintSnapshot`] over the area it was painted in. It takes
/// no space in layout and receives no input.
pub fn snapshot(
    snapshot: PaintSnapshot,
    effect: impl Fn(&PaintSnapshot) -> PaintEffect + 'static,
) -> crate::Canvas<()> {
    crate::canvas(
        |_, _, _| {},
        move |_, _, window, _| window.paint_snapshot(&snapshot, &effect),
    )
}
