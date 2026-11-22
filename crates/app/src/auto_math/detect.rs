//! Geometric `=` detector, spatial equation stroke gatherer, and handwriting style extractor.
use super::glyphs::HandwritingStyle;
use crate::Session;
use folio_document::*;
use std::collections::HashSet;

/// A candidate handwritten equation ending with a freshly drawn `=` sign.
#[derive(Clone)]
pub struct EqualsCandidate {
    pub eq_ids: (Id, Id),
    /// Stroke IDs gathered for the LHS (excluding the two `=` bars).
    pub lhs_ids: Vec<Id>,
    pub eq_bounds: Rect,
    pub answer_x: f32,
    pub style: HandwritingStyle,
    /// Ordered stroke polylines (LHS strokes in writing order, followed by the two `=` bars).
    pub strokes: Vec<Vec<[f32; 2]>>,
    /// Deterministic seed derived from the equation geometry for subtle per-glyph variation.
    pub seed: u64,
}

#[derive(Clone, Copy)]
struct BarMetrics {
    id: Id,
    bounds: Rect,
    cx: f32,
    cy: f32,
    w: f32,
    slope: f32,
    order: usize,
}

fn transformed_points(stroke: &InkStroke) -> Vec<Point> {
    stroke
        .display_path()
        .iter()
        .map(|p| stroke.transform.apply(p.position))
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .collect()
}

fn bar_metrics(stroke: &InkStroke, order: usize) -> Option<BarMetrics> {
    if stroke.style.tool == InkTool::Highlighter {
        return None;
    }
    let pts = transformed_points(stroke);
    if pts.len() < 2 {
        return None;
    }
    let bounds = Rect::from_points(pts.iter().copied());
    let w = bounds.width();
    let h = bounds.height();
    // Allow natural hand slant (~30°) while rejecting vertical strokes and tall shapes.
    if w < 6.0 || w < h * 1.25 {
        return None;
    }
    let first = pts[0];
    let last = *pts.last().unwrap();
    let raw_dx = last.x - first.x;
    let raw_dy = last.y - first.y;
    let dx = raw_dx.abs();
    let dy = raw_dy.abs();
    if dx < 5.0 || dy > dx * 0.58 {
        return None;
    }
    let chord = first.distance(last);
    let mut path_len = 0.0_f32;
    for pair in pts.windows(2) {
        path_len += pair[0].distance(pair[1]);
    }
    if path_len < 1e-3 || chord < w * 0.70 || chord / path_len < 0.72 {
        return None;
    }
    let slope = if raw_dx >= 0.0 {
        raw_dy / dx
    } else {
        -raw_dy / dx
    };
    Some(BarMetrics {
        id: stroke.id,
        bounds,
        cx: bounds.center().x,
        cy: bounds.center().y,
        w,
        slope,
        order,
    })
}

fn bars_form_equals(a: BarMetrics, b: BarMetrics) -> bool {
    let min_w = a.w.min(b.w);
    let max_w = a.w.max(b.w);
    if min_w / max_w < 0.35 {
        return false;
    }
    // The two bars of `=` should run roughly parallel.
    if (a.slope - b.slope).abs() > 0.45 {
        return false;
    }
    let x_overlap = a.bounds.max.x.min(b.bounds.max.x) - a.bounds.min.x.max(b.bounds.min.x);
    if x_overlap < min_w * 0.30 {
        return false;
    }
    if (a.cx - b.cx).abs() > max_w * 0.48 {
        return false;
    }
    let avg_w = 0.5 * (a.w + b.w);
    let dy = (a.cy - b.cy).abs();
    dy >= 3.0 && dy >= avg_w * 0.12 && dy <= avg_w * 1.45
}

/// Check whether the horizontal band immediately to the right of `eq_bounds` is empty.
pub fn right_side_is_clear(
    session: &Session,
    eq_ids: (Id, Id),
    eq_bounds: Rect,
    glyph_height: f32,
) -> bool {
    let page = session.page();
    let hidden = page.hidden_sources();
    let eq_w = eq_bounds.width().max(8.0);
    let eq_cy = eq_bounds.center().y;
    let check_rect = Rect::new(
        eq_bounds.max.x + 1.5,
        eq_cy - glyph_height * 0.42,
        (eq_w * 3.0).max(glyph_height * 1.8),
        glyph_height * 0.84,
    );
    for id in session.index.query(check_rect) {
        if id == eq_ids.0 || id == eq_ids.1 || hidden.contains(&id) {
            continue;
        }
        let Some(obj) = page.objects.get(&id) else {
            continue;
        };
        if matches!(obj.as_ref(), Object::Stroke(s) if s.style.tool == InkTool::Highlighter) {
            continue;
        }
        let b = obj.bounds();
        if b.max.x > eq_bounds.max.x + 2.0 && b.intersects(check_rect) {
            return false;
        }
    }
    true
}

/// Detect whether freshly committed `stroke` completes an `=` sign at the end of a handwritten
/// expression, and if so gather the expression's strokes and writing style.
///
/// `ignored_bar_ids` excludes strokes that cannot form a new `=` bar (such as previously
/// generated answers and already-solved `=` bars). `consumed_lhs_ids` excludes strokes that
/// already belonged to previously solved equations so tightly stacked rows never bleed together.
pub fn detect_equals_equation(
    session: &Session,
    stroke: &InkStroke,
    ignored_bar_ids: &HashSet<Id>,
    consumed_lhs_ids: &HashSet<Id>,
) -> Option<EqualsCandidate> {
    let page = session.page();
    let hidden = page.hidden_sources();
    let s2_order = session.order_positions.get(&stroke.id).copied()?;
    let bar2 = bar_metrics(stroke, s2_order)?;

    // Search nearby for a matching parallel horizontal bar forming `=`.
    let search = bar2.bounds.expand((bar2.w * 1.65).max(26.0));
    let mut partners: Vec<BarMetrics> = Vec::new();
    for id in session.index.query(search) {
        if id == stroke.id
            || hidden.contains(&id)
            || ignored_bar_ids.contains(&id)
            || consumed_lhs_ids.contains(&id)
        {
            continue;
        }
        let Some(obj) = page.objects.get(&id) else {
            continue;
        };
        let Object::Stroke(other) = obj.as_ref() else {
            continue;
        };
        let Some(&ord) = session.order_positions.get(&id) else {
            continue;
        };
        if let Some(m) = bar_metrics(other, ord)
            && bars_form_equals(bar2, m)
        {
            partners.push(m);
        }
    }
    if partners.is_empty() {
        return None;
    }
    // Sort closest vertically first (preferring the most recently drawn bar on ties).
    partners.sort_by(|a, b| {
        (a.cy - bar2.cy)
            .abs()
            .total_cmp(&(b.cy - bar2.cy).abs())
            .then_with(|| b.order.cmp(&a.order))
    });
    let bar1 = partners[0];
    // Reject `≡` (three evenly-stacked bars on the same symbol), while still allowing
    // a separate equation's bar sitting farther away on an adjacent row.
    if partners.len() >= 2 {
        let d0 = (bar1.cy - bar2.cy).abs();
        let d1 = (partners[1].cy - bar2.cy).abs();
        let same_triple = (bar1.cy < bar2.cy) != (partners[1].cy < bar2.cy)
            && d1 <= d0 * 1.65
            && (bar1.bounds.min.x.max(partners[1].bounds.min.x)
                <= bar1.bounds.max.x.min(partners[1].bounds.max.x));
        if same_triple {
            return None;
        }
    }
    let eq_ids = (bar1.id, bar2.id);
    let eq_bounds = bar1.bounds.union(bar2.bounds);
    let eq_w = eq_bounds.width().max(8.0);
    let eq_h = eq_bounds.height().max(4.0);
    let eq_cy = eq_bounds.center().y;

    // Gather candidate strokes to the left of `=` on roughly the same horizontal row.
    let initial_band = (eq_h * 1.9).max(eq_w * 0.95).max(24.0);
    let max_reach_left = (eq_w * 28.0).max(900.0);
    let query_rect = Rect::new(
        eq_bounds.min.x - max_reach_left,
        eq_cy - initial_band * 3.0,
        max_reach_left + eq_w * 0.35,
        initial_band * 6.0,
    );

    struct CandStroke<'a> {
        id: Id,
        order: usize,
        bounds: Rect,
        stroke: &'a InkStroke,
        pts: Vec<Point>,
        is_bar: bool,
    }

    let mut cands: Vec<CandStroke<'_>> = Vec::new();
    for id in session.index.query(query_rect) {
        if id == eq_ids.0
            || id == eq_ids.1
            || hidden.contains(&id)
            || consumed_lhs_ids.contains(&id)
        {
            continue;
        }
        let Some(obj) = page.objects.get(&id) else {
            continue;
        };
        let Object::Stroke(st) = obj.as_ref() else {
            continue;
        };
        if st.style.tool == InkTool::Highlighter {
            continue;
        }
        let pts = transformed_points(st);
        if pts.is_empty() {
            continue;
        }
        let b = Rect::from_points(pts.iter().copied());
        // Only strokes mostly to the left of the equals sign's center.
        if b.min.x > eq_bounds.center().x {
            continue;
        }
        let order = session.order_positions.get(&id).copied().unwrap_or(0);
        let is_bar = bar_metrics(st, order).is_some();
        cands.push(CandStroke {
            id,
            order,
            bounds: b,
            stroke: st,
            pts,
            is_bar,
        });
    }
    if cands.is_empty() {
        return None;
    }

    // Estimate local character height strictly from nearby strokes overlapping the `=` midline band.
    let near_x = eq_bounds.min.x - (eq_w * 6.0).max(140.0);
    let mut near_heights: Vec<f32> = cands
        .iter()
        .filter(|c| {
            c.bounds.max.x >= near_x
                && c.bounds.min.y <= eq_cy + initial_band * 0.75
                && c.bounds.max.y >= eq_cy - initial_band * 0.75
                && c.bounds.height() >= 6.0
                && !c.is_bar
        })
        .map(|c| c.bounds.height())
        .collect();
    near_heights.sort_by(f32::total_cmp);
    let char_h = if !near_heights.is_empty() {
        near_heights[near_heights.len() / 2].clamp((eq_h * 1.2).max(14.0), (eq_h * 5.0).max(120.0))
    } else {
        (eq_h * 2.2).max(eq_w * 1.1).clamp(16.0, 96.0)
    };

    // Abort immediately if the user already has content to the right of `=`.
    if !right_side_is_clear(session, eq_ids, eq_bounds, char_h) {
        return None;
    }

    // Pass 1: Grow the main horizontal equation row right-to-left, strictly anchored to `eq_cy`
    // so tightly spaced equations on adjacent rows NEVER bleed into this expression.
    cands.sort_by(|a, b| b.bounds.max.x.total_cmp(&a.bounds.max.x));
    // Pre-identify any earlier `=` bar pairs to the left so growth never bridges across them.
    let mut earlier_eq_bars = vec![false; cands.len()];
    for i in 0..cands.len() {
        if !cands[i].is_bar || cands[i].bounds.max.x >= eq_bounds.min.x - eq_w * 0.4 {
            continue;
        }
        if let Some(bm) = bar_metrics(cands[i].stroke, cands[i].order) {
            for j in (i + 1)..cands.len() {
                if !cands[j].is_bar {
                    continue;
                }
                if let Some(om) = bar_metrics(cands[j].stroke, cands[j].order)
                    && bars_form_equals(bm, om)
                {
                    earlier_eq_bars[i] = true;
                    earlier_eq_bars[j] = true;
                }
            }
        }
    }

    let max_h_gap = (char_h * 2.4).max(eq_w * 2.6);
    let row_core_y0 = eq_cy - char_h * 0.46;
    let row_core_y1 = eq_cy + char_h * 0.46;
    // Hard horizontal barrier at any earlier `=` sign on the same row so Pass 1 cannot hop over it.
    let same_row_eq_barrier = cands
        .iter()
        .enumerate()
        .filter(|&(i, c)| {
            earlier_eq_bars[i]
                && c.bounds.center().y >= eq_cy - char_h * 0.70
                && c.bounds.center().y <= eq_cy + char_h * 0.70
        })
        .map(|(_, c)| c.bounds.max.x)
        .reduce(f32::max);
    let mut selected: Vec<usize> = Vec::new();
    let mut picked = vec![false; cands.len()];
    let mut cluster_x0 = eq_bounds.min.x;

    let mut progress = true;
    while progress && selected.len() < 128 {
        progress = false;
        for (i, c) in cands.iter().enumerate() {
            if picked[i] || earlier_eq_bars[i] {
                continue;
            }
            if same_row_eq_barrier.is_some_and(|bx| c.bounds.center().x <= bx) {
                continue;
            }
            let h_gap = (cluster_x0 - c.bounds.max.x).max(0.0);
            if h_gap > max_h_gap {
                continue;
            }
            let cy = c.bounds.center().y;
            let overlaps_row_core = c.bounds.max.y >= row_core_y0 && c.bounds.min.y <= row_core_y1;
            let center_in_row = cy >= eq_cy - char_h * 0.56 && cy <= eq_cy + char_h * 0.56;
            if overlaps_row_core || center_in_row {
                picked[i] = true;
                selected.push(i);
                cluster_x0 = cluster_x0.min(c.bounds.min.x);
                progress = true;
                if selected.len() >= 128 {
                    break;
                }
            }
        }
    }

    if selected.is_empty() {
        return None;
    }

    // Pass 2: Attach genuine 2D math components (fraction numerators/denominators over a true
    // fraction bar that has ink both above and below it, plus compact superscripts/subscripts/dots
    // tightly adjacent to a collected base stroke) WITHOUT ever pulling in an adjacent row.
    let main_row = selected.clone();
    // Identify genuine fraction bars in `main_row`: horizontal bars not crossed by a `+` stem
    // that have at least one stroke above and at least one stroke below within their x-span.
    let fraction_bars: Vec<usize> = main_row
        .iter()
        .copied()
        .filter(|&bi| {
            let bar = &cands[bi];
            if !bar.is_bar || bar.bounds.width() < char_h * 0.42 {
                return false;
            }
            let crossed_by_plus = main_row.iter().any(|&oi| {
                if oi == bi {
                    return false;
                }
                let o = &cands[oi];
                o.bounds.height() > bar.bounds.height() * 1.6
                    && o.bounds.min.y < bar.bounds.min.y - 2.0
                    && o.bounds.max.y > bar.bounds.max.y + 2.0
                    && o.bounds.center().x >= bar.bounds.min.x
                    && o.bounds.center().x <= bar.bounds.max.x
            });
            if crossed_by_plus {
                return false;
            }
            let mut has_above = false;
            let mut has_below = false;
            for (ci, c) in cands.iter().enumerate() {
                if ci == bi || earlier_eq_bars[ci] {
                    continue;
                }
                let cx = c.bounds.center().x;
                if cx < bar.bounds.min.x - char_h * 0.25 || cx > bar.bounds.max.x + char_h * 0.25 {
                    continue;
                }
                let above_gap = bar.bounds.min.y - c.bounds.max.y;
                let below_gap = c.bounds.min.y - bar.bounds.max.y;
                if c.bounds.center().y < bar.bounds.min.y
                    && above_gap >= -4.0
                    && above_gap <= char_h * 0.72
                {
                    has_above = true;
                }
                if c.bounds.center().y > bar.bounds.max.y
                    && below_gap >= -4.0
                    && below_gap <= char_h * 0.72
                {
                    has_below = true;
                }
            }
            has_above && has_below
        })
        .collect();

    for (i, c) in cands.iter().enumerate() {
        if picked[i] || earlier_eq_bars[i] || selected.len() >= 128 {
            continue;
        }
        // Only attach strokes horizontally within the equation's x-span (with small margin).
        if c.bounds.max.x < cluster_x0 - char_h * 0.4 || c.bounds.min.x > eq_bounds.min.x {
            continue;
        }
        let cx = c.bounds.center().x;
        let attach_fraction = fraction_bars.iter().any(|&bi| {
            let bar = &cands[bi];
            let in_bar_x = cx >= bar.bounds.min.x - char_h * 0.25
                && cx <= bar.bounds.max.x + char_h * 0.25;
            if !in_bar_x {
                return false;
            }
            let above_gap = bar.bounds.min.y - c.bounds.max.y;
            let below_gap = c.bounds.min.y - bar.bounds.max.y;
            (above_gap >= -4.0 && above_gap <= char_h * 0.72)
                || (below_gap >= -4.0 && below_gap <= char_h * 0.72)
        });
        let is_tiny_dot =
            c.bounds.height() <= char_h * 0.28 && c.bounds.width() <= char_h * 0.28;
        let attach_dot = !attach_fraction
            && is_tiny_dot
            && main_row.iter().any(|&bi| {
                let base = &cands[bi];
                let h_near = c.bounds.min.x <= base.bounds.max.x + char_h * 0.25
                    && c.bounds.max.x >= base.bounds.min.x - char_h * 0.25;
                if !h_near {
                    return false;
                }
                let above_gap = base.bounds.min.y - c.bounds.max.y;
                let below_gap = c.bounds.min.y - base.bounds.max.y;
                (above_gap >= -char_h * 0.25 && above_gap <= char_h * 0.30)
                    || (below_gap >= -char_h * 0.25 && below_gap <= char_h * 0.30)
            });
        let attach_script = !attach_fraction
            && !attach_dot
            && c.bounds.height() <= char_h * 0.70
            && c.bounds.width() <= char_h * 0.90
            && main_row.iter().any(|&bi| {
                let base = &cands[bi];
                // Exponents/subscripts sit to the upper-right or lower-right of their base stroke.
                let h_right = cx >= base.bounds.center().x
                    && c.bounds.min.x <= base.bounds.max.x + char_h * 0.45;
                if !h_right {
                    return false;
                }
                let above_gap = base.bounds.min.y - c.bounds.max.y;
                let below_gap = c.bounds.min.y - base.bounds.max.y;
                (above_gap >= -char_h * 0.35 && above_gap <= char_h * 0.22)
                    || (below_gap >= -char_h * 0.35 && below_gap <= char_h * 0.22)
            });
        if attach_fraction || attach_dot || attach_script {
            picked[i] = true;
            selected.push(i);
        }
    }

    // Sort LHS strokes in natural writing order, then append the two `=` strokes.
    selected.sort_by_key(|&i| cands[i].order);
    let lhs_ids: Vec<Id> = selected.iter().map(|&i| cands[i].id).collect();

    // Extract user handwriting metrics (glyph height, slant, pressure) from the gathered LHS strokes.
    let mut heights: Vec<f32> = selected
        .iter()
        .map(|&i| &cands[i])
        .filter(|c| !c.is_bar && c.bounds.height() >= 6.0)
        .map(|c| c.bounds.height())
        .collect();
    heights.sort_by(f32::total_cmp);
    let glyph_height = if !heights.is_empty() {
        // Use upper-median (~60th percentile) so tall digits aren't shrunk by small symbols like `+`.
        let p60 = heights[(heights.len() * 6) / 10];
        p60.clamp((eq_h * 1.35).max(16.0), 120.0)
    } else {
        char_h.clamp(16.0, 100.0)
    };

    let mut slants: Vec<f32> = Vec::new();
    let mut pressures: Vec<f32> = Vec::new();
    for &i in &selected {
        let c = &cands[i];
        for sp in c.stroke.raw.iter() {
            if sp.pressure.is_finite() && sp.pressure > 0.05 {
                pressures.push(sp.pressure);
            }
        }
        if c.pts.len() >= 2 && c.bounds.height() >= glyph_height * 0.45 && !c.is_bar {
            let top = c.pts.iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
            let bot = c.pts.iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
            let dy = bot.y - top.y;
            if dy >= glyph_height * 0.40 {
                let s = (top.x - bot.x) / dy;
                if s.is_finite() && s.abs() <= 0.42 {
                    slants.push(s);
                }
            }
        }
    }
    for sp in stroke.raw.iter() {
        if sp.pressure.is_finite() && sp.pressure > 0.05 {
            pressures.push(sp.pressure);
        }
    }
    slants.sort_by(f32::total_cmp);
    let slant = if !slants.is_empty() {
        slants[slants.len() / 2].clamp(-0.20, 0.25)
    } else {
        0.06
    };
    pressures.sort_by(f32::total_cmp);
    let pressure = if !pressures.is_empty() {
        pressures[pressures.len() / 2].clamp(0.30, 0.88)
    } else {
        0.55
    };

    let mut strokes_out: Vec<Vec<[f32; 2]>> = Vec::with_capacity(selected.len() + 2);
    for &i in &selected {
        strokes_out.push(cands[i].pts.iter().map(|p| [p.x, p.y]).collect());
    }
    let eq_pair = if bar1.order <= bar2.order {
        [bar1.id, bar2.id]
    } else {
        [bar2.id, bar1.id]
    };
    for id in eq_pair {
        if let Some(Object::Stroke(st)) = page.objects.get(&id).map(|o| o.as_ref()) {
            let pts: Vec<[f32; 2]> = transformed_points(st).into_iter().map(|p| [p.x, p.y]).collect();
            if !pts.is_empty() {
                strokes_out.push(pts);
            }
        }
    }

    let seed = (eq_bounds.min.x.to_bits() as u64)
        ^ ((eq_bounds.min.y.to_bits() as u64) << 17)
        ^ ((selected.len() as u64) << 33);
    let answer_x = eq_bounds.max.x + (glyph_height * 0.44).max(eq_w * 0.55);

    Some(EqualsCandidate {
        eq_ids,
        lhs_ids,
        eq_bounds,
        answer_x,
        style: HandwritingStyle {
            pen: stroke.style.clone(),
            glyph_height,
            midline_y: eq_cy,
            slant,
            pressure,
        },
        strokes: strokes_out,
        seed,
    })
}
