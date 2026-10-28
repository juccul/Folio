//! Pointer input: starting, updating and finishing an [`Interaction`].
use super::*;

pub(crate) const INK_HOLD_DELAY: Duration = Duration::from_millis(550);
const INK_ENDPOINT_SLOP: f32 = 4.;
const SHAPE_PREVIEW_SLOP: f32 = 16.;

pub enum Interaction {
    Ink {
        builder: StrokeBuilder,
        last_move: Instant,
        anchor: Point,
        fit_attempted: bool,
        preview: Option<folio_shapes::Fit>,
        suppress_tap: bool,
    },
    Erase {
        ids: HashSet<Id>,
        last: Point,
        fragments: HashMap<Id, Vec<InkStroke>>,
    },
    Lasso {
        points: Vec<Point>,
    },
    Rectangle {
        start: Point,
        end: Point,
    },
    Move {
        start: Point,
        end: Point,
    },
    Resize {
        anchor: Point,
        start: Point,
        end: Point,
    },
    Rotate {
        center: Point,
        start: Point,
        end: Point,
    },
    Pan {
        start: Point,
        pan: Point,
    },
}
impl Interaction {
    fn update_ink_endpoint(&mut self, p: Point, zoom: f32) {
        if let Self::Ink {
            last_move,
            anchor,
            fit_attempted,
            preview,
            suppress_tap,
            ..
        } = self
        {
            // Once snapped, tolerate release drift without relaxing the initial
            // hold detection. Keep the anchor fixed so slow travel can still exit.
            let slop = if preview.is_some() {
                SHAPE_PREVIEW_SLOP
            } else {
                INK_ENDPOINT_SLOP
            };
            if p.distance(*anchor) * zoom > slop {
                *last_move = Instant::now();
                *anchor = p;
                *preview = None;
                *fit_attempted = false;
                *suppress_tap = false;
            }
        }
    }
}

impl Controller {
    pub fn cancel(&mut self) {
        if let Some(Interaction::Ink { builder, .. }) = &self.interaction {
            self.clear_ink_draft(builder.id());
        }
        self.interaction = None;
        self.pending_text = None;
        self.pending_text_edit = None;
        self.text_click = None;
        self.cursor = None;
    }
    /// Route one pen, touch or mouse event to the active interaction.
    pub fn pointer(&mut self, event: PenEvent) {
        // A tab can be selected before its background load completes. Never
        // apply that tab's input to the document that is still displayed.
        if self.loading_note() {
            return;
        }
        if event.phase == Phase::Down && self.tool != Tool::Hand {
            self.finish();
            self.focus_page_at(event.position);
        }
        let p = self.session().viewport.to_document(event.position);
        self.cursor = Some(p);
        match event.phase {
            Phase::Down => {
                if self.begin_interaction(&event, p) {
                    self.update_interaction(&event, p);
                }
            }
            Phase::Move => self.update_interaction(&event, p),
            Phase::Up => self.end_interaction(&event, p),
            Phase::Leave => {
                if self.interaction.is_some() {
                    self.finish()
                }
                self.cursor = None
            }
            Phase::Cancel => self.cancel(),
            Phase::Hover => {}
        }
    }
    /// Start an interaction for a press at document point `p`. Returns whether
    /// the new interaction should also receive the press as its first sample.
    fn begin_interaction(&mut self, event: &PenEvent, p: Point) -> bool {
        self.finish();
        if self.session().document.metadata.trashed && self.tool != Tool::Hand {
            self.status = "In Trash · restore this document to edit".into();
            return false;
        }
        if self.tool == Tool::Hand {
            self.interaction = Some(Interaction::Pan {
                start: event.position,
                pan: self.session().viewport.pan,
            });
            return false;
        }
        let properties = &self.page().properties;
        if !properties.infinite
            && !Rect::new(0., 0., properties.width, properties.height).contains(p)
        {
            return false;
        }
        // The eraser end of a pen and its secondary button always erase.
        let mut tool = if event.tool == PenTool::Eraser || event.buttons & 2 != 0 {
            Tool::Eraser
        } else {
            self.tool
        };
        if matches!(tool, Tool::Lasso | Tool::Rectangle)
            && let Some(handle) = self.selection_handle_at(p)
        {
            self.interaction = Some(handle);
            return false;
        }
        self.text_click = None;
        if let Some(id) = self.text_box_at(p, tool, event.device) {
            // A second click on the only selected text box edits it on release.
            if self.session().selection.len() == 1 && self.session().selection.contains(&id) {
                self.text_click = Some((id, p));
            }
            self.session_mut().selection = HashSet::from([id]);
            self.tool = Tool::Lasso;
            self.temporary_selection = false;
            self.interaction = Some(Interaction::Move { start: p, end: p });
            return false;
        }
        let selection_hit = self
            .selection_bounds()
            .is_some_and(|r| r.expand(8.).contains(p));
        // Tapping away from a gesture-made selection returns to the pen, but
        // that tap itself must not leave a dot.
        let dismiss_selection = self.temporary_selection && tool == Tool::Lasso && !selection_hit;
        if dismiss_selection {
            self.session_mut().selection.clear();
            self.tool = Tool::Pen;
            self.temporary_selection = false;
            tool = Tool::Pen;
        }
        if tool == Tool::Pen {
            self.begin_bootstrap_writing();
        }
        self.interaction = match tool {
            Tool::Pen | Tool::Shape => {
                let mut builder = StrokeBuilder::new(self.style.clone());
                builder.push(event.sample(p));
                Some(Interaction::Ink {
                    builder,
                    last_move: Instant::now(),
                    anchor: p,
                    fit_attempted: false,
                    preview: None,
                    suppress_tap: dismiss_selection,
                })
            }
            Tool::Eraser => Some(Interaction::Erase {
                ids: HashSet::new(),
                last: p,
                fragments: HashMap::new(),
            }),
            Tool::Lasso | Tool::Rectangle if selection_hit => {
                Some(Interaction::Move { start: p, end: p })
            }
            Tool::Lasso => {
                self.session_mut().selection.clear();
                Some(Interaction::Lasso { points: vec![p] })
            }
            Tool::Rectangle => {
                self.session_mut().selection.clear();
                Some(Interaction::Rectangle { start: p, end: p })
            }
            Tool::Text => {
                self.pending_text = Some(p);
                None
            }
            Tool::Hand => None,
        };
        true
    }
    /// A resize corner or the rotation handle of the current selection.
    fn selection_handle_at(&self, p: Point) -> Option<Interaction> {
        let bounds = self.selection_bounds()?;
        let zoom = self.session().viewport.zoom;
        let corners = [
            bounds.min,
            Point::new(bounds.max.x, bounds.min.y),
            bounds.max,
            Point::new(bounds.min.x, bounds.max.y),
        ];
        if let Some((i, corner)) = corners
            .iter()
            .enumerate()
            .find(|(_, corner)| corner.distance(p) < 10. / zoom)
        {
            return Some(Interaction::Resize {
                anchor: corners[(i + 2) % 4],
                start: *corner,
                end: p,
            });
        }
        let handle = Point::new(bounds.center().x, bounds.min.y - 24. / zoom);
        (p.distance(handle) < 10. / zoom).then(|| Interaction::Rotate {
            center: bounds.center(),
            start: p,
            end: p,
        })
    }
    /// The topmost text box under `p`, when this tool and device select text.
    fn text_box_at(&self, p: Point, tool: Tool, device: folio_input::Device) -> Option<Id> {
        let selects_text = !matches!(tool, Tool::Eraser | Tool::Hand | Tool::Shape)
            && (device == folio_input::Device::Mouse
                || matches!(tool, Tool::Text | Tool::Lasso | Tool::Rectangle));
        if !selects_text {
            return None;
        }
        let session = self.session();
        session
            .index
            .query(Rect::new(p.x, p.y, 0., 0.))
            .into_iter()
            .filter_map(|id| {
                let Object::Text(t) = session.page().objects.get(&id)?.as_ref() else {
                    return None;
                };
                let inverse = t.transform.inverse()?;
                t.rect
                    .contains(inverse.apply(p))
                    .then(|| (session.order_positions.get(&id).copied().unwrap_or(0), t.id))
            })
            .max_by_key(|(rank, _)| *rank)
            .map(|(_, id)| id)
    }
    fn clamp_to_page(&self, p: Point) -> Point {
        let properties = &self.page().properties;
        if properties.infinite {
            p
        } else {
            Point::new(
                p.x.clamp(0., properties.width),
                p.y.clamp(0., properties.height),
            )
        }
    }
    /// Objects the eraser touches while moving from `from` to `to`.
    fn eraser_hits(&self, from: Point, to: Point) -> Vec<Id> {
        let radius = self.settings.eraser_radius;
        let whole_objects = self.settings.effective_eraser_mode() == EraserMode::Object;
        let page = self.page();
        self.session()
            .index
            .query(Rect::from_points([from, to]).expand(radius))
            .into_iter()
            .filter(|id| {
                page.objects.get(id).is_some_and(|o| match o.as_ref() {
                    Object::Stroke(s) => folio_ink::swept_hit(s, from, to, radius),
                    _ => whole_objects && o.bounds().expand(radius).contains(to),
                })
            })
            .collect()
    }
    fn update_interaction(&mut self, event: &PenEvent, p: Point) {
        let zoom = self.session().viewport.zoom;
        if event.phase == Phase::Move
            && self
                .text_click
                .is_some_and(|(_, start)| start.distance(p) > 4. / zoom)
        {
            self.text_click = None;
        }
        let p = self.clamp_to_page(p);
        let erased = match &self.interaction {
            Some(Interaction::Erase { last, .. }) => self.eraser_hits(*last, p),
            _ => Vec::new(),
        };
        let eraser_radius = self.settings.eraser_radius;
        let segment_eraser = self.settings.effective_eraser_mode() == EraserMode::Segment;
        let Some(active) = self.interaction.as_mut() else {
            return;
        };
        active.update_ink_endpoint(p, zoom);
        match active {
            Interaction::Ink { builder, .. } => {
                if event.phase == Phase::Move {
                    builder.push(event.sample(p));
                }
            }
            Interaction::Erase {
                ids,
                last,
                fragments,
            } => {
                if segment_eraser {
                    let page = self.sessions[&self.active].page();
                    for id in &erased {
                        let Some(Object::Stroke(stroke)) = page.objects.get(id).map(|o| o.as_ref())
                        else {
                            continue;
                        };
                        // Cut the remaining fragments of strokes touched earlier.
                        let parts = fragments.remove(id).unwrap_or_else(|| vec![stroke.clone()]);
                        let parts = parts
                            .iter()
                            .flat_map(|part| folio_ink::cut_segment(part, *last, p, eraser_radius))
                            .collect();
                        fragments.insert(*id, parts);
                    }
                }
                ids.extend(erased);
                *last = p;
            }
            Interaction::Lasso { points } => push_lasso_point(points, p),
            Interaction::Rectangle { end, .. }
            | Interaction::Move { end, .. }
            | Interaction::Resize { end, .. }
            | Interaction::Rotate { end, .. } => *end = p,
            Interaction::Pan { start, pan } => {
                let new = Point::new(
                    pan.x + event.position.x - start.x,
                    pan.y + event.position.y - start.y,
                );
                self.session_mut().viewport.pan = new;
            }
        }
    }
    fn end_interaction(&mut self, event: &PenEvent, p: Point) {
        let zoom = self.session().viewport.zoom;
        // Releasing a second click on a selected text box opens its editor.
        if let Some((id, start)) = self.text_click.take()
            && start.distance(p) <= 4. / zoom
            && matches!(self.interaction, Some(Interaction::Move { .. }))
        {
            self.interaction = None;
            self.pending_text_edit = Some(id);
            return;
        }
        if let Some(interaction) = &mut self.interaction {
            interaction.update_ink_endpoint(p, zoom);
            match interaction {
                Interaction::Ink { builder, .. } => {
                    if builder
                        .raw()
                        .last()
                        .is_some_and(|last| last.position().distance(p) > 0.12)
                    {
                        builder.push(event.sample(p));
                    }
                }
                Interaction::Lasso { points } => push_lasso_point(points, p),
                Interaction::Move { end, .. }
                | Interaction::Resize { end, .. }
                | Interaction::Rotate { end, .. }
                | Interaction::Rectangle { end, .. } => *end = p,
                _ => {}
            }
        }
        self.finish();
    }
    /// Complete the active interaction, committing whatever it produced.
    pub fn finish(&mut self) {
        let Some(active) = self.interaction.take() else {
            return;
        };
        match active {
            Interaction::Ink {
                builder,
                preview,
                last_move,
                suppress_tap,
                ..
            } => {
                if suppress_tap {
                    self.clear_ink_draft(builder.id());
                } else if let Some(stroke) = builder.finish() {
                    self.finish_stroke(stroke, preview, last_move.elapsed() >= INK_HOLD_DELAY);
                }
            }
            Interaction::Erase { ids, fragments, .. } => self.finish_erase(&ids, &fragments),
            Interaction::Lasso { points } => self.select_polygon(&points),
            Interaction::Rectangle { start, end } => {
                self.select_rectangle(Rect::from_points([start, end]))
            }
            Interaction::Move { start, end } => self.transform_selection(
                Transform::translate(end.x - start.x, end.y - start.y),
                "Move selection",
            ),
            Interaction::Resize { anchor, start, end } => {
                self.resize_selection(resize_transform(anchor, start, end), "Resize selection")
            }
            Interaction::Rotate { center, start, end } => {
                self.transform_selection(rotation_transform(center, start, end), "Rotate selection")
            }
            Interaction::Pan { .. } => {}
        }
    }
    /// A finished pen stroke is, in order of precedence: an encircle-select
    /// gesture, a scratch-erase gesture, or ink (optionally fitted to a shape).
    fn finish_stroke(&mut self, stroke: InkStroke, preview: Option<folio_shapes::Fit>, held: bool) {
        let points = std::cell::OnceCell::new();
        let points =
            || points.get_or_init(|| stroke.raw.iter().map(|p| p.position()).collect::<Vec<_>>());
        let smart_pen = self.tool == Tool::Pen && stroke.style.tool != InkTool::Highlighter;
        // A deliberate endpoint hold identifies selection intent. Narrow,
        // overdrawn ovals can also resemble a scratch; never erase their
        // enclosed ink before honoring that selection.
        if smart_pen && self.settings.encircle_select && held {
            let ids = self.encircled_ids(points());
            if !ids.is_empty() {
                self.session_mut().selection = ids;
                self.tool = Tool::Lasso;
                self.temporary_selection = true;
                self.clear_ink_draft(stroke.id);
                return;
            }
        }
        if smart_pen && self.settings.scratch_erase {
            let ids = self.scratch_erase_targets(&stroke);
            if !ids.is_empty() {
                self.clear_ink_draft(stroke.id);
                self.delete_ids(&ids, "Scratch erase");
                return;
            }
        }
        let shape_requested =
            self.tool == Tool::Shape || smart_pen && self.settings.hold_shapes && held;
        let fit = preview.clone().or_else(|| {
            shape_requested
                .then(|| folio_shapes::fit(points()))
                .flatten()
        });
        self.commit_stroke(stroke, fit, shape_requested || preview.is_some());
    }
    /// Objects crossed out by a scratch gesture, or nothing if `stroke` is not one.
    fn scratch_erase_targets(&self, stroke: &InkStroke) -> HashSet<Id> {
        let Some(gesture) = folio_gestures::scratch(&stroke.raw) else {
            return HashSet::new();
        };
        let page = self.page();
        let index = &self.session().index;
        let objects = index
            .query(gesture.bounds.expand(3.))
            .iter()
            .filter_map(|id| page.objects.get(id).map(|object| object.as_ref()))
            .collect::<Vec<_>>();
        let mut ids = gesture
            .targets(&objects)
            .into_iter()
            .collect::<HashSet<_>>();
        // Also remove detached marks (dots, crossbars) of crossed-out letters.
        let crossed = ids
            .iter()
            .filter_map(|id| page.objects.get(id))
            .filter(
                |o| matches!(o.as_ref(), Object::Stroke(s) if s.style.tool != InkTool::Highlighter),
            )
            .map(|o| o.as_ref())
            .collect::<Vec<_>>();
        for body in &crossed {
            for id in index.query(gesture.letter_search_bounds(body)) {
                if page
                    .objects
                    .get(&id)
                    .is_some_and(|o| gesture.completes_letter(o, std::slice::from_ref(body)))
                {
                    ids.insert(id);
                }
            }
        }
        ids
    }
    /// Commit `stroke`, plus a shape fitted to it when there is one. With shape
    /// intent, a line drawn from an existing line can join it into an arrow.
    fn commit_stroke(
        &mut self,
        stroke: InkStroke,
        mut fit: Option<folio_shapes::Fit>,
        shape_intent: bool,
    ) {
        let joined = if shape_intent
            && fit
                .as_ref()
                .is_none_or(|f| matches!(f.kind, ShapeKind::Line | ShapeKind::Polyline))
        {
            self.joined_arrow(&stroke)
        } else {
            None
        };
        if let Some(joined) = &joined {
            fit = Some(joined.fit.clone());
        }
        let id = stroke.id;
        let index = self.page().order.len();
        let page = self.page().id;
        let mut changes = vec![Change::Object {
            page,
            id,
            before: None,
            after: Some(Arc::new(Object::Stroke(stroke.clone()))),
            index,
        }];
        let mut sources = vec![id];
        if let Some(joined) = joined {
            sources.splice(0..0, joined.sources);
            if matches!(joined.previous.as_ref(), Object::Shape(_)) {
                let previous_id = joined.previous.id();
                changes.push(Change::Object {
                    page,
                    id: previous_id,
                    before: Some(joined.previous),
                    after: None,
                    index: self.session().order_positions[&previous_id],
                });
            }
        }
        if let Some(fit) = fit {
            let shape = Shape {
                id: Id::new_v4(),
                kind: fit.kind,
                vertices: fit.vertices,
                style: stroke.style,
                transform: Transform::default(),
                source_strokes: sources,
            };
            changes.push(Change::Object {
                page,
                id: shape.id,
                before: None,
                after: Some(Arc::new(Object::Shape(shape))),
                index: index + 1,
            });
        }
        self.commit("Draw stroke", changes);
    }
    fn finish_erase(&mut self, ids: &HashSet<Id>, fragments: &HashMap<Id, Vec<InkStroke>>) {
        if self.settings.effective_eraser_mode() != EraserMode::Segment {
            self.delete_ids(ids, "Erase strokes");
            return;
        }
        // Replace each touched stroke with its surviving fragments, in place.
        let page = self.page();
        let mut changes = vec![];
        for (index, id) in self.ordered_selection(ids) {
            if let Some(before) = page.objects.get(&id) {
                changes.push(Change::Object {
                    page: page.id,
                    id,
                    before: Some(before.clone()),
                    after: None,
                    index,
                });
                if let Some(parts) = fragments.get(&id) {
                    for (offset, part) in parts.iter().enumerate() {
                        changes.push(Change::Object {
                            page: page.id,
                            id: part.id,
                            before: None,
                            after: Some(Arc::new(Object::Stroke(part.clone()))),
                            index: index + offset,
                        });
                    }
                }
            }
        }
        self.commit("Erase segments", changes);
    }
    fn clear_ink_draft(&mut self, stroke: Id) {
        self.persistence
            .draft(self.active, self.page().id, stroke, None);
    }
    /// Save the in-progress stroke about once a second so a crash keeps it.
    pub(crate) fn save_ink_draft(&mut self) {
        if !self.settings.autosave
            || self.provisional_imports.contains(&self.active)
            || self.last_draft.elapsed() <= Duration::from_secs(1)
        {
            return;
        }
        if let Some(Interaction::Ink {
            builder,
            suppress_tap: false,
            ..
        }) = &self.interaction
            && let Some(stroke) = builder.snapshot()
        {
            self.persistence.draft(
                self.active,
                self.page().id,
                stroke.id,
                Some(Arc::new(Object::Stroke(stroke))),
            );
        }
        self.last_draft = Instant::now();
    }
    /// Holding the pen still at the end of a stroke either completes an
    /// encircle selection immediately or shows a shape-fit preview.
    pub(crate) fn detect_ink_hold(&mut self) -> bool {
        let smart_pen = self.tool == Tool::Pen && self.style.tool != InkTool::Highlighter;
        let watching = self.settings.hold_shapes && self.tool == Tool::Shape
            || smart_pen && (self.settings.hold_shapes || self.settings.encircle_select);
        let Some(Interaction::Ink {
            builder,
            last_move,
            fit_attempted: false,
            suppress_tap: false,
            ..
        }) = &self.interaction
        else {
            return false;
        };
        if !watching || last_move.elapsed() < INK_HOLD_DELAY {
            return false;
        }
        let points = builder
            .raw()
            .iter()
            .map(|p| p.position())
            .collect::<Vec<_>>();
        if smart_pen && self.settings.encircle_select && !self.encircled_ids(&points).is_empty() {
            self.finish();
            return true;
        }
        let hold_shapes = self.settings.hold_shapes;
        let Some(Interaction::Ink {
            preview,
            fit_attempted,
            anchor,
            ..
        }) = &mut self.interaction
        else {
            return false;
        };
        *fit_attempted = true;
        if !hold_shapes {
            return false;
        }
        *preview = folio_shapes::fit(&points);
        if preview.is_some()
            && let Some(endpoint) = points.last()
        {
            *anchor = *endpoint;
        }
        preview.is_some()
    }
    pub(crate) fn encircled_ids(&self, points: &[Point]) -> HashSet<Id> {
        let Some(polygon) =
            folio_gestures::selection_loop_with_scale(points, self.session().viewport.zoom)
        else {
            return HashSet::new();
        };
        self.session()
            .index
            .query(Rect::from_points(polygon.iter().copied()).expand(2.))
            .into_iter()
            .filter(|id| {
                self.page()
                    .objects
                    .get(id)
                    .is_some_and(|o| folio_gestures::encloses(&polygon, o))
            })
            .collect()
    }
    fn select_polygon(&mut self, polygon: &[Point]) {
        let bounds = Rect::from_points(polygon.iter().copied());
        let page = self.page();
        let hidden = page.hidden_sources();
        let ids = self
            .session()
            .index
            .query(bounds)
            .into_iter()
            .filter(|id| {
                !hidden.contains(id)
                    && page.objects.get(id).is_some_and(|o| {
                        let r = o.bounds();
                        folio_ink::inside_polygon(r.center(), polygon)
                            || [r.min, r.max]
                                .iter()
                                .any(|p| folio_ink::inside_polygon(*p, polygon))
                    })
            })
            .collect();
        self.session_mut().selection = ids;
    }
    fn select_rectangle(&mut self, r: Rect) {
        let page = self.page();
        let hidden = page.hidden_sources();
        let ids = self
            .session()
            .index
            .query(r)
            .into_iter()
            .filter(|id| {
                !hidden.contains(id)
                    && page
                        .objects
                        .get(id)
                        .is_some_and(|o| r.intersects(o.bounds()))
            })
            .collect();
        self.session_mut().selection = ids;
    }
    pub fn interaction_transform(&self) -> Transform {
        match &self.interaction {
            Some(Interaction::Move { start, end }) => {
                Transform::translate(end.x - start.x, end.y - start.y)
            }
            Some(Interaction::Resize { anchor, start, end }) => {
                resize_transform(*anchor, *start, *end)
            }
            Some(Interaction::Rotate { center, start, end }) => {
                rotation_transform(*center, *start, *end)
            }
            _ => Transform::default(),
        }
    }
    pub fn preview_text(&self, text: &TextBlock) -> TextBlock {
        let mut text = text.clone();
        if self.session().selection.contains(&text.id) {
            if matches!(self.interaction, Some(Interaction::Resize { .. })) {
                text.reflow(self.interaction_transform());
            } else {
                text.transform = self.interaction_transform().compose(text.transform);
            }
        }
        text
    }
}
fn push_lasso_point(points: &mut Vec<Point>, p: Point) {
    if points.last().is_none_or(|last| last.distance(p) > 1.) {
        points.push(p);
    }
}
/// Scale about `anchor` so that `start` moves to `end`, per axis.
fn resize_transform(anchor: Point, start: Point, end: Point) -> Transform {
    let ratio = |original: f32, current: f32| {
        if original.abs() < 0.001 {
            1.
        } else {
            (current / original).clamp(0.05, 20.)
        }
    };
    let sx = ratio(start.x - anchor.x, end.x - anchor.x);
    let sy = ratio(start.y - anchor.y, end.y - anchor.y);
    Transform::translate(anchor.x, anchor.y)
        .compose(Transform {
            a: sx,
            d: sy,
            ..Transform::default()
        })
        .compose(Transform::translate(-anchor.x, -anchor.y))
}
/// Rotate about `center` by the angle swept from `start` to `end`.
fn rotation_transform(center: Point, start: Point, end: Point) -> Transform {
    let angle =
        (end.y - center.y).atan2(end.x - center.x) - (start.y - center.y).atan2(start.x - center.x);
    Transform::around(center, 1., angle)
}
