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

/// A handwritten equation on the page that already has ink on both sides of `=`
/// (e.g. `x = 3` or `2x + 4 = 10`), used lazily to resolve variables in later rows.
#[derive(Clone)]
pub struct PageEquationCandidate {
    /// Ordered stroke polylines covering LHS + `=` + RHS for one isolated row.
    pub strokes: Vec<Vec<[f32; 2]>>,
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

/// Evaluate the local y-coordinate of a horizontal/sloped/arched bar polyline at `x`.
fn bar_y_at_x(pts: &[Point], x: f32) -> f32 {
    if pts.is_empty() {
        return 0.0;
    }
    if pts.len() == 1 {
        return pts[0].y;
    }
    let mut best_y = pts[0].y;
    let mut best_dx = f32::INFINITY;
    for pair in pts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let lo = a.x.min(b.x);
        let hi = a.x.max(b.x);
        if x >= lo && x <= hi {
            let span = b.x - a.x;
            let t = if span.abs() > 1e-4 {
                ((x - a.x) / span).clamp(0.0, 1.0)
            } else {
                0.5
            };
            return a.y + t * (b.y - a.y);
        }
        let d = (x - a.x).abs();
        if d < best_dx {
            best_dx = d;
            best_y = a.y;
        }
        let d2 = (x - b.x).abs();
        if d2 < best_dx {
            best_dx = d2;
            best_y = b.y;
        }
    }
    best_y
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
    // Allow compact bars and natural hand slant (~33°) while rejecting vertical strokes and loops.
    if w < 4.5 || w < h * 1.15 {
        return None;
    }
    let first = pts[0];
    let last = *pts.last().unwrap();
    let raw_dx = last.x - first.x;
    let raw_dy = last.y - first.y;
    let dx = raw_dx.abs();
    let dy = raw_dy.abs();
    if dx < 3.8 || dy > dx * 0.65 {
        return None;
    }
    let chord = first.distance(last);
    let mut path_len = 0.0_f32;
    for pair in pts.windows(2) {
        path_len += pair[0].distance(pair[1]);
    }
    if path_len < 1e-3 || chord < w * 0.72 || chord / path_len < 0.80 {
        return None;
    }
    // Reject bent shapes like `√` whose interior dips far away from the start->end chord.
    let inv_chord = 1.0 / chord.max(1e-3);
    let max_dev = pts
        .iter()
        .map(|p| ((p.x - first.x) * raw_dy - (p.y - first.y) * raw_dx).abs() * inv_chord)
        .fold(0.0_f32, f32::max);
    if max_dev > (w * 0.24).max(4.0) {
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
    if min_w / max_w < 0.32 {
        return false;
    }
    // Allow natural slight opposing wrist slant (e.g. top bar down-right, bottom bar up-right).
    if (a.slope - b.slope).abs() > 0.78 {
        return false;
    }
    let x_overlap = a.bounds.max.x.min(b.bounds.max.x) - a.bounds.min.x.max(b.bounds.min.x);
    if x_overlap < min_w * 0.25 {
        return false;
    }
    if (a.cx - b.cx).abs() > max_w * 0.52 {
        return false;
    }
    let avg_w = 0.5 * (a.w + b.w);
    let dy = (a.cy - b.cy).abs();
    dy >= 2.5 && dy >= avg_w * 0.10 && dy <= avg_w * 2.25
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
    let search = bar2.bounds.expand((bar2.w * 2.4).max(34.0));
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
            // Ensure no non-bar stroke sits strictly between the two bars (e.g. fraction numerator
            // between a cap bar and a fraction bar, or middle digit between stacked fraction bars).
            let top_y = bar2.cy.min(m.cy);
            let bot_y = bar2.cy.max(m.cy);
            let x0 = bar2.bounds.min.x.min(m.bounds.min.x);
            let x1 = bar2.bounds.max.x.max(m.bounds.max.x);
            let between_rect = Rect::new(x0, top_y, (x1 - x0).max(1.0), (bot_y - top_y).max(1.0));
            let has_ink_between = session.index.query(between_rect).into_iter().any(|bid| {
                if bid == stroke.id || bid == id || hidden.contains(&bid) {
                    return false;
                }
                let Some(Object::Stroke(st)) = page.objects.get(&bid).map(|o| o.as_ref()) else {
                    return false;
                };
                if st.style.tool == InkTool::Highlighter {
                    return false;
                }
                let b_ord = session.order_positions.get(&bid).copied().unwrap_or(0);
                if bar_metrics(st, b_ord).is_some() {
                    return false;
                }
                let b = st.bounds();
                let c = b.center();
                c.x >= x0 && c.x <= x1 && c.y > top_y && c.y < bot_y
            });
            if !has_ink_between {
                partners.push(m);
            }
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

    // Estimate local character height from nearby non-bar strokes overlapping the `=` midline band.
    // If a long fraction bar is the only stroke at `eq_cy`, fall back to a slightly wider vertical
    // window so numerator/denominator digits still inform `char_h`.
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
    if near_heights.is_empty() {
        near_heights = cands
            .iter()
            .filter(|c| {
                c.bounds.max.x >= near_x
                    && c.bounds.min.y <= eq_cy + initial_band * 1.8
                    && c.bounds.max.y >= eq_cy - initial_band * 1.8
                    && c.bounds.height() >= 6.0
                    && !c.is_bar
            })
            .map(|c| c.bounds.height())
            .collect();
    }
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
    // Two parallel fraction bars with an expression sitting between them (like `\frac{4}{\frac{4}{4}}`)
    // are NOT an `=` sign.
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
                    let top_y = bm.cy.min(om.cy);
                    let bot_y = bm.cy.max(om.cy);
                    let x0 = bm.bounds.min.x.min(om.bounds.min.x);
                    let x1 = bm.bounds.max.x.max(om.bounds.max.x);
                    let has_ink_between = cands.iter().enumerate().any(|(k, c)| {
                        k != i
                            && k != j
                            && !c.is_bar
                            && c.bounds.center().x >= x0
                            && c.bounds.center().x <= x1
                            && c.bounds.center().y > top_y
                            && c.bounds.center().y < bot_y
                    });
                    // Allow an inner `=` inside parenthesized probability queries like `P(X = 3) =`
                    // (bounded by a tall `(` on the left and a tall `)` right before the outer `=`).
                    let pair_cy = 0.5 * (top_y + bot_y);
                    let enclosed_in_parens = (pair_cy - eq_cy).abs() <= char_h * 0.70
                        && (eq_bounds.min.x - x1) <= char_h * 5.0
                        && cands.iter().any(|c| {
                            !c.is_bar
                                && c.bounds.min.x >= x1
                                && c.bounds.max.x <= eq_bounds.min.x
                                && (eq_bounds.min.x - c.bounds.max.x) <= char_h * 1.4
                                && c.bounds.height() >= char_h * 0.80
                                && c.bounds.width() <= c.bounds.height() * 0.65
                                && (c.bounds.center().y - eq_cy).abs() <= char_h * 0.65
                        })
                        && cands.iter().any(|c| {
                            !c.is_bar
                                && c.bounds.max.x <= x0
                                && (x0 - c.bounds.max.x) <= char_h * 3.5
                                && c.bounds.height() >= char_h * 0.80
                                && c.bounds.width() <= c.bounds.height() * 0.65
                                && (c.bounds.center().y - eq_cy).abs() <= char_h * 0.65
                        });
                    if !has_ink_between && !enclosed_in_parens {
                        earlier_eq_bars[i] = true;
                        earlier_eq_bars[j] = true;
                    }
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

    // Pass 2: Iteratively attach genuine 2D math components WITHOUT pulling in adjacent rows:
    //   1) True fraction bars (both Pass-1 bars and nested fraction bars immediately above/below
    //      a collected stroke that have ink on BOTH sides and are not `+` crossbars),
    //   2) Fraction numerators/denominators measured against the bar's local curve `bar_y_at_x`
    //      so long sloped/arched fraction bars never drop outer terms,
    //   3) Compact superscripts/subscripts/dots plus horizontal multi-digit script continuation
    //      (e.g. `10` in `100^{10}` or `10` in `\log_{10}(100)`).
    let main_row = selected.clone();
    let is_true_fraction_bar = |bi: usize, cands: &[CandStroke<'_>], earlier_eq_bars: &[bool]| -> bool {
        let bar = &cands[bi];
        if !bar.is_bar || earlier_eq_bars[bi] || bar.bounds.width() < char_h * 0.42 {
            return false;
        }
        let crossed_by_plus = cands.iter().enumerate().any(|(oi, o)| {
            if oi == bi || earlier_eq_bars[oi] {
                return false;
            }
            let cx = o.bounds.center().x;
            if cx < bar.bounds.min.x || cx > bar.bounds.max.x {
                return false;
            }
            let local_y = bar_y_at_x(&bar.pts, cx);
            o.bounds.height() > bar.bounds.height() * 1.6
                && o.bounds.min.y < local_y - 2.5
                && o.bounds.max.y > local_y + 2.5
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
            if cx < bar.bounds.min.x - char_h * 0.30 || cx > bar.bounds.max.x + char_h * 0.30 {
                continue;
            }
            let local_y = bar_y_at_x(&bar.pts, cx);
            let above_gap = local_y - c.bounds.max.y;
            let below_gap = c.bounds.min.y - local_y;
            if c.bounds.center().y < local_y
                && above_gap >= -char_h * 0.25
                && above_gap <= char_h * 0.90
            {
                has_above = true;
            }
            if c.bounds.center().y > local_y
                && below_gap >= -char_h * 0.25
                && below_gap <= char_h * 0.90
            {
                has_below = true;
            }
        }
        has_above && has_below
    };

    let mut fraction_bars: Vec<usize> = main_row
        .iter()
        .copied()
        .filter(|&bi| is_true_fraction_bar(bi, &cands, &earlier_eq_bars))
        .collect();
    let mut is_frac_bar = vec![false; cands.len()];
    for &bi in &fraction_bars {
        is_frac_bar[bi] = true;
    }
    let mut attached_scripts: Vec<usize> = Vec::new();

    for _hop in 0..12 {
        let mut round_progress = false;

        // Discover nested fraction bars tightly above/below an already-selected stroke
        // (e.g. `\frac{4}{\frac{4}{4}} =` where `=` aligns with the middle `4`).
        let current_sel = selected.clone();
        for (i, c) in cands.iter().enumerate() {
            if picked[i] || is_frac_bar[i] || selected.len() >= 128 {
                continue;
            }
            if c.bounds.max.x < cluster_x0 - char_h * 0.5 || c.bounds.min.x > eq_bounds.min.x {
                continue;
            }
            if !is_true_fraction_bar(i, &cands, &earlier_eq_bars) {
                continue;
            }
            let borders_selected = current_sel.iter().any(|&si| {
                let s = &cands[si];
                let sx = s.bounds.center().x;
                if sx < c.bounds.min.x - char_h * 0.30 || sx > c.bounds.max.x + char_h * 0.30 {
                    return false;
                }
                let local_y = bar_y_at_x(&c.pts, sx);
                let above_gap = local_y - s.bounds.max.y;
                let below_gap = s.bounds.min.y - local_y;
                (above_gap >= -char_h * 0.25 && above_gap <= char_h * 0.85)
                    || (below_gap >= -char_h * 0.25 && below_gap <= char_h * 0.85)
            });
            if borders_selected {
                picked[i] = true;
                selected.push(i);
                cluster_x0 = cluster_x0.min(c.bounds.min.x);
                is_frac_bar[i] = true;
                fraction_bars.push(i);
                round_progress = true;
            }
        }

        // Attach fraction numerators/denominators, tiny dots, scripts, and multi-digit script tails.
        for (i, c) in cands.iter().enumerate() {
            if picked[i] || earlier_eq_bars[i] || selected.len() >= 128 {
                continue;
            }
            if c.bounds.max.x < cluster_x0 - char_h * 0.45 || c.bounds.min.x > eq_bounds.min.x {
                continue;
            }
            let cx = c.bounds.center().x;
            let cy = c.bounds.center().y;
            let attach_fraction = fraction_bars.iter().any(|&bi| {
                let bar = &cands[bi];
                let in_bar_x = cx >= bar.bounds.min.x - char_h * 0.30
                    && cx <= bar.bounds.max.x + char_h * 0.30;
                if !in_bar_x {
                    return false;
                }
                let local_y = bar_y_at_x(&bar.pts, cx);
                let above_gap = local_y - c.bounds.max.y;
                let below_gap = c.bounds.min.y - local_y;
                (cy < local_y && above_gap >= -char_h * 0.25 && above_gap <= char_h * 0.85)
                    || (cy > local_y && below_gap >= -char_h * 0.25 && below_gap <= char_h * 0.85)
            });
            let is_tiny_dot =
                c.bounds.height() <= char_h * 0.28 && c.bounds.width() <= char_h * 0.28;
            let attach_dot = !attach_fraction
                && is_tiny_dot
                && selected.iter().any(|&bi| {
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
            let compact_script =
                c.bounds.height() <= char_h * 0.78 && c.bounds.width() <= char_h * 0.95;
            let attach_script = !attach_fraction
                && !attach_dot
                && compact_script
                && main_row.iter().any(|&bi| {
                    let base = &cands[bi];
                    let h_right = cx >= base.bounds.center().x
                        && c.bounds.min.x <= base.bounds.max.x + char_h * 0.50;
                    if !h_right {
                        return false;
                    }
                    let above_gap = base.bounds.min.y - c.bounds.max.y;
                    let below_gap = c.bounds.min.y - base.bounds.max.y;
                    (above_gap >= -char_h * 0.35 && above_gap <= char_h * 0.28)
                        || (below_gap >= -char_h * 0.35 && below_gap <= char_h * 0.28)
                });
            let continue_script = !attach_fraction
                && !attach_dot
                && !attach_script
                && compact_script
                && attached_scripts.iter().any(|&si| {
                    let prev = &cands[si];
                    let h_gap = (c.bounds.min.x - prev.bounds.max.x)
                        .max(prev.bounds.min.x - c.bounds.max.x)
                        .max(0.0);
                    let same_level = (cy - prev.bounds.center().y).abs() <= char_h * 0.40;
                    h_gap <= char_h * 0.55 && same_level
                });
            // Character cap bars / crossbars (e.g. top bar of `T` in `Tan(...)`, top bar of `\pi`
            // both on the main row and inside a fraction numerator, `5`'s top bar, `t`/`7` crossbars).
            let is_cap_bar_shape = (c.is_bar
                || (c.bounds.height() <= char_h * 0.48
                    && c.bounds.width() >= c.bounds.height() * 1.15))
                && c.bounds.height() <= char_h * 0.50
                && c.bounds.width() <= char_h * 3.8;
            let attach_cap_or_crossbar = !attach_fraction
                && !attach_dot
                && !attach_script
                && !continue_script
                && is_cap_bar_shape
                && !is_true_fraction_bar(i, &cands, &earlier_eq_bars)
                && selected.iter().any(|&bi| {
                    let base = &cands[bi];
                    if base.is_bar || base.bounds.height() < char_h * 0.30 {
                        return false;
                    }
                    let base_cx = base.bounds.center().x;
                    if base_cx < c.bounds.min.x - char_h * 0.25
                        || base_cx > c.bounds.max.x + char_h * 0.25
                    {
                        return false;
                    }
                    let local_y = bar_y_at_x(&c.pts, base_cx);
                    // Crosses or touches the stem directly (`T`, `t`, `7`, `5`, `E`, `F`, `H`, `A`).
                    if local_y >= base.bounds.min.y - char_h * 0.12
                        && local_y <= base.bounds.max.y + char_h * 0.12
                    {
                        return true;
                    }
                    // Tightly caps the top of `base` (e.g. `\pi` or `T` with a slight pen-lift gap),
                    // guarded so we never steal a bottom bar belonging to a row directly above.
                    let above_gap = base.bounds.min.y - local_y;
                    if above_gap > 0.0 && above_gap <= char_h * 0.38 {
                        let closer_above = cands.iter().enumerate().any(|(oi, o)| {
                            !picked[oi]
                                && oi != i
                                && !o.is_bar
                                && o.bounds.center().x >= c.bounds.min.x - char_h * 0.20
                                && o.bounds.center().x <= c.bounds.max.x + char_h * 0.20
                                && o.bounds.max.y <= local_y + char_h * 0.12
                                && (local_y - o.bounds.max.y).max(0.0) < above_gap
                        });
                        return !closer_above;
                    }
                    // Tightly foots the bottom of `base` (e.g. `I`, `E`, `Z` bottom bar).
                    let below_gap = local_y - base.bounds.max.y;
                    if below_gap > 0.0 && below_gap <= char_h * 0.24 {
                        let closer_below = cands.iter().enumerate().any(|(oi, o)| {
                            !picked[oi]
                                && oi != i
                                && !o.is_bar
                                && o.bounds.center().x >= c.bounds.min.x - char_h * 0.20
                                && o.bounds.center().x <= c.bounds.max.x + char_h * 0.20
                                && o.bounds.min.y >= local_y - char_h * 0.12
                                && (o.bounds.min.y - local_y).max(0.0) < below_gap
                        });
                        return !closer_below;
                    }
                    false
                });
            if attach_fraction
                || attach_dot
                || attach_script
                || continue_script
                || attach_cap_or_crossbar
            {
                picked[i] = true;
                selected.push(i);
                cluster_x0 = cluster_x0.min(c.bounds.min.x);
                if attach_script
                    || continue_script
                    || (attach_dot
                        && attached_scripts.iter().any(|&si| {
                            let prev = &cands[si];
                            c.bounds.min.x <= prev.bounds.max.x + char_h * 0.35
                                && c.bounds.max.x >= prev.bounds.min.x - char_h * 0.35
                                && (cy - prev.bounds.center().y).abs() <= char_h * 0.50
                        }))
                {
                    attached_scripts.push(i);
                }
                if attach_fraction
                    && is_true_fraction_bar(i, &cands, &earlier_eq_bars)
                    && !is_frac_bar[i]
                {
                    is_frac_bar[i] = true;
                    fraction_bars.push(i);
                }
                round_progress = true;
            }
        }
        if !round_progress {
            break;
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

/// Scan the current page for handwritten equations that already have ink on BOTH sides of `=`
/// (such as `x = 3`, `y = 2x + 1`, or `2x + 4 = 10`). Returned in top-to-bottom reading order
/// so later variable reassignments override earlier ones.
///
/// Only called lazily by the background worker when a triggered equation contains an unresolved
/// variable, so pure numeric expressions incur zero extra recognition work.
pub fn detect_page_equations(
    session: &Session,
    exclude_eq_ids: (Id, Id),
    ignored_bar_ids: &HashSet<Id>,
) -> Vec<PageEquationCandidate> {
    let page = session.page();
    let hidden = page.hidden_sources();

    struct RowStroke {
        order: usize,
        bounds: Rect,
        pts: Vec<Point>,
        bar: Option<BarMetrics>,
    }

    let mut all: Vec<RowStroke> = Vec::new();
    for (order, id) in page.order.iter().enumerate() {
        if *id == exclude_eq_ids.0 || *id == exclude_eq_ids.1 || hidden.contains(id) {
            continue;
        }
        let Some(obj) = page.objects.get(id) else {
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
        let bounds = Rect::from_points(pts.iter().copied());
        let bar = if ignored_bar_ids.contains(id) {
            None
        } else {
            bar_metrics(st, order)
        };
        all.push(RowStroke {
            order,
            bounds,
            pts,
            bar,
        });
    }

    // Find disjoint `=` bar pairs on the page.
    let bar_indices: Vec<usize> = all
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.bar.map(|_| i))
        .collect();
    let mut used_bar = vec![false; all.len()];
    let mut eq_pairs: Vec<(usize, usize, Rect)> = Vec::new();
    for &i in &bar_indices {
        if used_bar[i] {
            continue;
        }
        let b1 = all[i].bar.unwrap();
        let mut best: Option<(usize, f32)> = None;
        for &j in &bar_indices {
            if j <= i || used_bar[j] {
                continue;
            }
            let b2 = all[j].bar.unwrap();
            if bars_form_equals(b1, b2) {
                let top_y = b1.cy.min(b2.cy);
                let bot_y = b1.cy.max(b2.cy);
                let x0 = b1.bounds.min.x.min(b2.bounds.min.x);
                let x1 = b1.bounds.max.x.max(b2.bounds.max.x);
                let has_ink_between = all.iter().enumerate().any(|(k, s)| {
                    k != i
                        && k != j
                        && s.bar.is_none()
                        && s.bounds.center().x >= x0
                        && s.bounds.center().x <= x1
                        && s.bounds.center().y > top_y
                        && s.bounds.center().y < bot_y
                });
                if !has_ink_between {
                    let dy = (b1.cy - b2.cy).abs();
                    if best.is_none_or(|(_, d)| dy < d) {
                        best = Some((j, dy));
                    }
                }
            }
        }
        if let Some((j, _)) = best {
            used_bar[i] = true;
            used_bar[j] = true;
            let bounds = b1.bounds.union(all[j].bar.unwrap().bounds);
            eq_pairs.push((i, j, bounds));
        }
    }

    // Sort top-to-bottom so later variable assignments naturally override earlier ones.
    eq_pairs.sort_by(|a, b| a.2.center().y.total_cmp(&b.2.center().y));

    let mut eq_bar_set: HashSet<usize> = HashSet::new();
    for &(i, j, _) in &eq_pairs {
        eq_bar_set.insert(i);
        eq_bar_set.insert(j);
    }

    let mut out: Vec<PageEquationCandidate> = Vec::new();
    for &(i1, i2, eq_bounds) in &eq_pairs {
        if out.len() >= 12 {
            break;
        }
        let eq_w = eq_bounds.width().max(8.0);
        let eq_h = eq_bounds.height().max(4.0);
        let eq_cy = eq_bounds.center().y;
        let band = (eq_h * 1.9).max(eq_w * 0.95).max(24.0);

        let mut heights: Vec<f32> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != i1
                    && idx != i2
                    && s.bar.is_none()
                    && s.bounds.height() >= 6.0
                    && s.bounds.min.y <= eq_cy + band * 0.8
                    && s.bounds.max.y >= eq_cy - band * 0.8
                    && (s.bounds.center().x - eq_bounds.center().x).abs() <= (eq_w * 7.0).max(160.0)
            })
            .map(|(_, s)| s.bounds.height())
            .collect();
        heights.sort_by(f32::total_cmp);
        let char_h = if !heights.is_empty() {
            heights[heights.len() / 2].clamp((eq_h * 1.2).max(14.0), (eq_h * 5.0).max(110.0))
        } else {
            (eq_h * 2.2).max(eq_w * 1.1).clamp(16.0, 96.0)
        };
        let row_y0 = eq_cy - char_h * 0.48;
        let row_y1 = eq_cy + char_h * 0.48;
        let max_h_gap = (char_h * 2.2).max(eq_w * 2.4);

        let in_row = |s: &RowStroke| {
            let cy = s.bounds.center().y;
            (s.bounds.max.y >= row_y0 && s.bounds.min.y <= row_y1)
                || (cy >= eq_cy - char_h * 0.56 && cy <= eq_cy + char_h * 0.56)
        };

        // Grow LHS right-to-left from `eq_bounds.min.x`.
        let mut lhs: Vec<usize> = Vec::new();
        let mut lhs_x0 = eq_bounds.min.x;
        let mut left_cands: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != i1
                    && idx != i2
                    && !eq_bar_set.contains(&idx)
                    && s.bounds.min.x < eq_bounds.center().x
                    && s.bounds.max.x <= eq_bounds.center().x + eq_w * 0.25
                    && in_row(s)
            })
            .map(|(idx, _)| idx)
            .collect();
        left_cands.sort_by(|&a, &b| all[b].bounds.max.x.total_cmp(&all[a].bounds.max.x));
        for idx in left_cands {
            let s = &all[idx];
            if (lhs_x0 - s.bounds.max.x).max(0.0) <= max_h_gap {
                lhs.push(idx);
                lhs_x0 = lhs_x0.min(s.bounds.min.x);
                if lhs.len() > 24 {
                    break;
                }
            }
        }
        if lhs.is_empty() || lhs.len() > 24 {
            continue;
        }

        // Grow RHS left-to-right from `eq_bounds.max.x`.
        let mut rhs: Vec<usize> = Vec::new();
        let mut rhs_x1 = eq_bounds.max.x;
        let mut right_cands: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != i1
                    && idx != i2
                    && !eq_bar_set.contains(&idx)
                    && s.bounds.max.x > eq_bounds.center().x
                    && s.bounds.min.x >= eq_bounds.center().x - eq_w * 0.25
                    && in_row(s)
            })
            .map(|(idx, _)| idx)
            .collect();
        right_cands.sort_by(|&a, &b| all[a].bounds.min.x.total_cmp(&all[b].bounds.min.x));
        for idx in right_cands {
            let s = &all[idx];
            if (s.bounds.min.x - rhs_x1).max(0.0) <= max_h_gap {
                rhs.push(idx);
                rhs_x1 = rhs_x1.max(s.bounds.max.x);
                if rhs.len() > 24 {
                    break;
                }
            }
        }
        if rhs.is_empty() || rhs.len() > 24 {
            continue;
        }

        // Pass 2 for page equations: attach superscripts/subscripts (`x = y^2 + 4`), fraction
        // numerators/denominators (`x = \frac{1}{2}`), dots, and cap bars (`T = 5`, `\pi`) on
        // each side without pulling in adjacent rows.
        let mut side_picked = vec![false; all.len()];
        side_picked[i1] = true;
        side_picked[i2] = true;
        for &idx in lhs.iter().chain(rhs.iter()) {
            side_picked[idx] = true;
        }
        let attach_side_2d = |side: &mut Vec<usize>,
                              side_x0: &mut f32,
                              side_x1: &mut f32,
                              is_lhs: bool,
                              side_picked: &mut [bool]| {
            let main_side = side.clone();
            let mut scripts: Vec<usize> = Vec::new();
            let mut frac_bars: Vec<usize> = side
                .iter()
                .copied()
                .filter(|&bi| {
                    let b = &all[bi];
                    if b.bar.is_none() || b.bounds.width() < char_h * 0.42 {
                        return false;
                    }
                    let mut has_above = false;
                    let mut has_below = false;
                    for (ci, c) in all.iter().enumerate() {
                        if ci == bi || eq_bar_set.contains(&ci) {
                            continue;
                        }
                        let cx = c.bounds.center().x;
                        if cx < b.bounds.min.x - char_h * 0.30 || cx > b.bounds.max.x + char_h * 0.30
                        {
                            continue;
                        }
                        let ly = bar_y_at_x(&b.pts, cx);
                        let ag = ly - c.bounds.max.y;
                        let bg = c.bounds.min.y - ly;
                        if c.bounds.center().y < ly && ag >= -char_h * 0.25 && ag <= char_h * 0.85 {
                            has_above = true;
                        }
                        if c.bounds.center().y > ly && bg >= -char_h * 0.25 && bg <= char_h * 0.85 {
                            has_below = true;
                        }
                    }
                    has_above && has_below
                })
                .collect();

            for _ in 0..2 {
                let mut progress = false;
                for (ci, c) in all.iter().enumerate() {
                    if side_picked[ci] || eq_bar_set.contains(&ci) || side.len() >= 24 {
                        continue;
                    }
                    let cx = c.bounds.center().x;
                    let cy = c.bounds.center().y;
                    if (is_lhs && cx >= eq_bounds.center().x)
                        || (!is_lhs && cx <= eq_bounds.center().x)
                    {
                        continue;
                    }
                    if c.bounds.max.x < *side_x0 - char_h * 0.45
                        || c.bounds.min.x > *side_x1 + char_h * 0.65
                    {
                        continue;
                    }
                    let attach_frac = frac_bars.iter().any(|&bi| {
                        let b = &all[bi];
                        if cx < b.bounds.min.x - char_h * 0.30 || cx > b.bounds.max.x + char_h * 0.30
                        {
                            return false;
                        }
                        let ly = bar_y_at_x(&b.pts, cx);
                        let ag = ly - c.bounds.max.y;
                        let bg = c.bounds.min.y - ly;
                        (cy < ly && ag >= -char_h * 0.25 && ag <= char_h * 0.85)
                            || (cy > ly && bg >= -char_h * 0.25 && bg <= char_h * 0.85)
                    });
                    let compact_script =
                        c.bounds.height() <= char_h * 0.82 && c.bounds.width() <= char_h * 0.98;
                    let attach_script = !attach_frac
                        && compact_script
                        && main_side.iter().any(|&bi| {
                            let base = &all[bi];
                            let h_right = cx >= base.bounds.center().x
                                && c.bounds.min.x <= base.bounds.max.x + char_h * 0.55;
                            if !h_right {
                                return false;
                            }
                            let ag = base.bounds.min.y - c.bounds.max.y;
                            let bg = c.bounds.min.y - base.bounds.max.y;
                            (ag >= -char_h * 0.35 && ag <= char_h * 0.32)
                                || (bg >= -char_h * 0.35 && bg <= char_h * 0.32)
                        });
                    let cont_script = !attach_frac
                        && !attach_script
                        && compact_script
                        && scripts.iter().any(|&si| {
                            let prev = &all[si];
                            let hg = (c.bounds.min.x - prev.bounds.max.x)
                                .max(prev.bounds.min.x - c.bounds.max.x)
                                .max(0.0);
                            hg <= char_h * 0.55 && (cy - prev.bounds.center().y).abs() <= char_h * 0.40
                        });
                    let attach_cap = !attach_frac
                        && !attach_script
                        && !cont_script
                        && (c.bar.is_some()
                            || (c.bounds.height() <= char_h * 0.48
                                && c.bounds.width() >= c.bounds.height() * 1.15))
                        && c.bounds.height() <= char_h * 0.50
                        && c.bounds.width() <= char_h * 3.8
                        && side.iter().any(|&bi| {
                            let base = &all[bi];
                            if base.bar.is_some() || base.bounds.height() < char_h * 0.30 {
                                return false;
                            }
                            let bcx = base.bounds.center().x;
                            if bcx < c.bounds.min.x - char_h * 0.25
                                || bcx > c.bounds.max.x + char_h * 0.25
                            {
                                return false;
                            }
                            let ly = bar_y_at_x(&c.pts, bcx);
                            let ag = base.bounds.min.y - ly;
                            (ly >= base.bounds.min.y - char_h * 0.12
                                && ly <= base.bounds.max.y + char_h * 0.12)
                                || (ag > 0.0 && ag <= char_h * 0.36)
                        });
                    if attach_frac || attach_script || cont_script || attach_cap {
                        side_picked[ci] = true;
                        side.push(ci);
                        *side_x0 = (*side_x0).min(c.bounds.min.x);
                        *side_x1 = (*side_x1).max(c.bounds.max.x);
                        if attach_script || cont_script {
                            scripts.push(ci);
                        }
                        if attach_frac && c.bar.is_some() {
                            frac_bars.push(ci);
                        }
                        progress = true;
                    }
                }
                if !progress {
                    break;
                }
            }
        };
        let mut lhs_x1 = eq_bounds.min.x;
        attach_side_2d(&mut lhs, &mut lhs_x0, &mut lhs_x1, true, &mut side_picked);
        let mut rhs_x0 = eq_bounds.max.x;
        attach_side_2d(&mut rhs, &mut rhs_x0, &mut rhs_x1, false, &mut side_picked);
        if lhs.len() > 24 || rhs.len() > 24 {
            continue;
        }

        let mut combined: Vec<usize> = Vec::with_capacity(lhs.len() + 2 + rhs.len());
        combined.extend(lhs);
        combined.push(i1);
        combined.push(i2);
        combined.extend(rhs);
        combined.sort_by_key(|&idx| all[idx].order);
        let strokes = combined
            .into_iter()
            .map(|idx| all[idx].pts.iter().map(|p| [p.x, p.y]).collect())
            .collect();
        out.push(PageEquationCandidate { strokes });
    }

    // Also scan for standalone `~` (`\sim`) distribution declarations on rows without `=`
    // (e.g. `X ~ B(10, 0.3)`, `X ~ Pois(4)`, `T ~ t_{10}`, `X ~ N(100, 15^2)`, `F ~ F(5, 10)`).
    let mut eq_row_bands: Vec<(f32, f32)> = eq_pairs
        .iter()
        .map(|&(_, _, b)| {
            let cy = b.center().y;
            let half = (b.height() * 1.4).max(18.0);
            (cy - half, cy + half)
        })
        .collect();
    let mut tilde_cands: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|&(idx, s)| {
            if eq_bar_set.contains(&idx) || s.pts.len() < 2 {
                return false;
            }
            let w = s.bounds.width();
            let h = s.bounds.height();
            if w < 5.5 || w < h * 1.05 || h > w * 0.95 {
                return false;
            }
            let first = s.pts[0];
            let last = *s.pts.last().unwrap();
            let dx = (last.x - first.x).abs();
            let dy = (last.y - first.y).abs();
            if dx < 4.5 || dy > dx * 0.72 {
                return false;
            }
            let cy = s.bounds.center().y;
            !eq_row_bands.iter().any(|&(y0, y1)| cy >= y0 && cy <= y1)
        })
        .map(|(idx, _)| idx)
        .collect();
    tilde_cands.sort_by(|&a, &b| all[a].bounds.center().y.total_cmp(&all[b].bounds.center().y));

    for tidx in tilde_cands {
        if out.len() >= 18 {
            break;
        }
        let t = &all[tidx];
        let t_cy = t.bounds.center().y;
        if eq_row_bands.iter().any(|&(y0, y1)| t_cy >= y0 && t_cy <= y1) {
            continue;
        }
        let t_w = t.bounds.width().max(8.0);
        let band = (t_w * 1.6).max(26.0);
        let mut heights: Vec<f32> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != tidx
                    && !eq_bar_set.contains(&idx)
                    && s.bar.is_none()
                    && s.bounds.height() >= 8.0
                    && s.bounds.min.y <= t_cy + band
                    && s.bounds.max.y >= t_cy - band
                    && (s.bounds.center().x - t.bounds.center().x).abs() <= (t_w * 8.0).max(180.0)
            })
            .map(|(_, s)| s.bounds.height())
            .collect();
        if heights.is_empty() {
            continue;
        }
        heights.sort_by(f32::total_cmp);
        let char_h = heights[heights.len() / 2].clamp(14.0, 110.0);
        let row_y0 = t_cy - char_h * 0.50;
        let row_y1 = t_cy + char_h * 0.50;
        let max_h_gap = (char_h * 2.2).max(t_w * 2.6);
        let in_row = |s: &RowStroke| {
            let cy = s.bounds.center().y;
            (s.bounds.max.y >= row_y0 && s.bounds.min.y <= row_y1)
                || (cy >= t_cy - char_h * 0.58 && cy <= t_cy + char_h * 0.58)
        };

        // Short random-variable LHS (1..=4 strokes, e.g. `X`, `T`, `\chi^2`).
        let mut lhs: Vec<usize> = Vec::new();
        let mut lhs_x0 = t.bounds.min.x;
        let mut left_cands: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != tidx
                    && !eq_bar_set.contains(&idx)
                    && s.bounds.max.x <= t.bounds.center().x
                    && in_row(s)
            })
            .map(|(idx, _)| idx)
            .collect();
        left_cands.sort_by(|&a, &b| all[b].bounds.max.x.total_cmp(&all[a].bounds.max.x));
        for idx in left_cands {
            let s = &all[idx];
            if (lhs_x0 - s.bounds.max.x).max(0.0) <= max_h_gap {
                lhs.push(idx);
                lhs_x0 = lhs_x0.min(s.bounds.min.x);
                if lhs.len() > 5 {
                    break;
                }
            }
        }
        if lhs.is_empty() || lhs.len() > 5 {
            continue;
        }

        // Distribution RHS (e.g. `B(10, 0.3)`, `Pois(3.2)`, `t_{10}`, `N(0, 1)`).
        let mut rhs: Vec<usize> = Vec::new();
        let mut rhs_x1 = t.bounds.max.x;
        let mut right_cands: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|&(idx, s)| {
                idx != tidx
                    && !eq_bar_set.contains(&idx)
                    && s.bounds.min.x >= t.bounds.center().x
                    && in_row(s)
            })
            .map(|(idx, _)| idx)
            .collect();
        right_cands.sort_by(|&a, &b| all[a].bounds.min.x.total_cmp(&all[b].bounds.min.x));
        for idx in right_cands {
            let s = &all[idx];
            if (s.bounds.min.x - rhs_x1).max(0.0) <= max_h_gap {
                rhs.push(idx);
                rhs_x1 = rhs_x1.max(s.bounds.max.x);
                if rhs.len() > 24 {
                    break;
                }
            }
        }
        if rhs.len() < 2 || rhs.len() > 24 {
            continue;
        }

        // Attach compact superscripts/subscripts/dots/cap-bars on this declaration row (e.g. `t_{10}`, `15^2`, `0.3`).
        let mut picked = vec![false; all.len()];
        picked[tidx] = true;
        for &idx in lhs.iter().chain(rhs.iter()) {
            picked[idx] = true;
        }
        let mut row_ids: Vec<usize> = Vec::with_capacity(lhs.len() + 1 + rhs.len());
        row_ids.extend(&lhs);
        row_ids.push(tidx);
        row_ids.extend(&rhs);
        let mut row_x0 = lhs_x0;
        let mut row_x1 = rhs_x1;
        for _ in 0..4 {
            let mut progress = false;
            for (ci, c) in all.iter().enumerate() {
                if picked[ci] || eq_bar_set.contains(&ci) || row_ids.len() >= 30 {
                    continue;
                }
                if c.bounds.max.x < row_x0 - char_h * 0.40 || c.bounds.min.x > row_x1 + char_h * 0.65
                {
                    continue;
                }
                let cx = c.bounds.center().x;
                let attach = row_ids.iter().any(|&bi| {
                    let base = &all[bi];
                    let h_near = c.bounds.min.x <= base.bounds.max.x + char_h * 0.55
                        && c.bounds.max.x >= base.bounds.min.x - char_h * 0.30;
                    if !h_near {
                        return false;
                    }
                    let is_dot = c.bounds.height() <= char_h * 0.30 && c.bounds.width() <= char_h * 0.30;
                    let is_script =
                        c.bounds.height() <= char_h * 0.80 && c.bounds.width() <= char_h * 0.95 && cx >= base.bounds.center().x;
                    let is_cap = c.bar.is_some()
                        && c.bounds.height() <= char_h * 0.45
                        && base.bar.is_none()
                        && base.bounds.center().x >= c.bounds.min.x - char_h * 0.25
                        && base.bounds.center().x <= c.bounds.max.x + char_h * 0.25;
                    let ag = base.bounds.min.y - c.bounds.max.y;
                    let bg = c.bounds.min.y - base.bounds.max.y;
                    (is_dot || is_script || is_cap)
                        && ((ag >= -char_h * 0.35 && ag <= char_h * 0.34)
                            || (bg >= -char_h * 0.35 && bg <= char_h * 0.30))
                });
                if attach {
                    picked[ci] = true;
                    row_ids.push(ci);
                    row_x0 = row_x0.min(c.bounds.min.x);
                    row_x1 = row_x1.max(c.bounds.max.x);
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
        row_ids.sort_by_key(|&idx| all[idx].order);
        let strokes = row_ids
            .into_iter()
            .map(|idx| all[idx].pts.iter().map(|p| [p.x, p.y]).collect())
            .collect();
        out.push(PageEquationCandidate { strokes });
        eq_row_bands.push((t_cy - char_h * 0.65, t_cy + char_h * 0.65));
    }

    out
}
