use folio_document::Point;

pub(crate) fn distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    if length < 1e-10 {
        return p.distance(a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length;
    p.distance(a.lerp(b, t.clamp(0., 1.)))
}
pub(crate) fn clean(points: &[Point]) -> Option<Vec<Point>> {
    let mut out: Vec<Point> = vec![];
    for &p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if out.last().is_none_or(|last| last.distance(p) > 0.001) {
            out.push(p);
        }
    }
    (out.len() >= 2).then_some(out)
}
pub(crate) fn length(points: &[Point]) -> f32 {
    points.windows(2).map(|p| p[0].distance(p[1])).sum()
}
pub(crate) fn resample(points: &[Point], count: usize) -> Vec<Point> {
    let total = length(points);
    let mut result = vec![points[0]];
    let mut index = 0;
    let mut traversed = 0.;
    for i in 1..count - 1 {
        let at = total * i as f32 / (count - 1) as f32;
        while index + 2 < points.len() && traversed + points[index].distance(points[index + 1]) < at
        {
            traversed += points[index].distance(points[index + 1]);
            index += 1;
        }
        let a = points[index];
        let b = points[index + 1];
        result.push(a.lerp(
            b,
            ((at - traversed) / a.distance(b).max(1e-8)).clamp(0., 1.),
        ));
    }
    result.push(*points.last().unwrap());
    result
}
pub(crate) fn axis(points: &[Point]) -> Point {
    let mean = mean(points);
    let (mut xx, mut yy, mut xy) = (0., 0., 0.);
    for p in points {
        let x = p.x - mean.x;
        let y = p.y - mean.y;
        xx += x * x;
        yy += y * y;
        xy += x * y;
    }
    let angle = 0.5 * (2. * xy).atan2(xx - yy);
    Point::new(angle.cos(), angle.sin())
}
pub(crate) fn mean(points: &[Point]) -> Point {
    Point::new(
        points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32,
        points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32,
    )
}
pub(crate) fn simplify(points: &[Point], epsilon: f32) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let first = points[0];
    let last = *points.last().unwrap();
    let (i, error) = points
        .iter()
        .enumerate()
        .skip(1)
        .take(points.len() - 2)
        .map(|(i, p)| (i, distance(*p, first, last)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    if error <= epsilon {
        return vec![first, last];
    }
    let mut result = simplify(&points[..=i], epsilon);
    result.pop();
    result.extend(simplify(&points[i..], epsilon));
    result
}
pub(crate) fn closed_corners(points: &[Point], epsilon: f32) -> Vec<Point> {
    let far = points
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| points[0].distance(**a).total_cmp(&points[0].distance(**b)))
        .unwrap()
        .0;
    let mut result = simplify(&points[..=far], epsilon);
    result.pop();
    result.extend(simplify(&points[far..], epsilon));
    if result.last().is_some_and(|p| p.distance(result[0]) < 0.20) {
        result.pop();
    }
    // A starting point in the middle of an edge, or a small closure hook, must
    // not manufacture a fifth corner in an otherwise clear rectangle.
    loop {
        if result.len() <= 3 {
            break;
        }
        let count = result.len();
        let removable = (0..count).find(|&i| {
            distance(
                result[i],
                result[(i + count - 1) % count],
                result[(i + 1) % count],
            ) < epsilon
        });
        if let Some(i) = removable {
            result.remove(i);
        } else {
            break;
        }
    }
    result
}
pub(crate) fn residual(points: &[Point], vertices: &[Point], closed: bool) -> (f32, f32) {
    let edges = vertices.len() - usize::from(!closed);
    let mut errors = points
        .iter()
        .map(|&p| {
            (0..edges)
                .map(|i| distance(p, vertices[i], vertices[(i + 1) % vertices.len()]))
                .fold(f32::INFINITY, f32::min)
        })
        .collect::<Vec<_>>();
    let mean = errors.iter().sum::<f32>() / errors.len() as f32;
    errors.sort_by(f32::total_cmp);
    (mean, errors[(errors.len() - 1) * 95 / 100])
}
