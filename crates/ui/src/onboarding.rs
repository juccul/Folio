//! The tour of Folio's smart ink: a full-window walk through what the pen
//! can do, each step playing a clip of the real app doing it (recorded by
//! `scripts/record-tour.py`, handwriting laid out from the auto-math glyph
//! samples), over a grid of dots that ripple under the pointer, shown once on
//! first launch and from Help and Settings.
use super::*;
use std::sync::mpsc;
use std::time::Instant;

/// The open tour: which step, which way the last step change went, the
/// clip it plays, when it opened and the step began (its entrances play on
/// those clocks) and the rippling dots behind it all.
pub(super) struct Tour {
    step: usize,
    /// The step before, whose buttons morph into this one's.
    from: usize,
    forward: bool,
    clip: Option<Clip>,
    opened: Instant,
    stepped: Instant,
    ripples: Ripples,
}

struct Step {
    title: &'static str,
    body: &'static str,
    /// Its name in the row of steps.
    short: &'static str,
    /// The clip as recorded in dark and in light.
    clip: [&'static [u8]; 2],
}

const STEPS: &[Step] = &[
    Step {
        title: "Welcome to Folio",
        body: "A notebook that understands your handwriting. Here is what your pen can do.",
        short: "Write",
        clip: [
            include_bytes!("../assets/tour/write-dark.webp"),
            include_bytes!("../assets/tour/write-light.webp"),
        ],
    },
    Step {
        title: "Circle to select",
        body: "Draw a loop around ink and hold the pen still. It is selected, ready to move, copy or convert.",
        short: "Select",
        clip: [
            include_bytes!("../assets/tour/encircle-dark.webp"),
            include_bytes!("../assets/tour/encircle-light.webp"),
        ],
    },
    Step {
        title: "Scratch out to erase",
        body: "Scribble over a word to erase it. Undo brings it back.",
        short: "Erase",
        clip: [
            include_bytes!("../assets/tour/scratch-dark.webp"),
            include_bytes!("../assets/tour/scratch-light.webp"),
        ],
    },
    Step {
        title: "Hold for perfect shapes",
        body: "Hold at the end of a stroke and a rough circle or line snaps into shape.",
        short: "Shapes",
        clip: [
            include_bytes!("../assets/tour/snap-dark.webp"),
            include_bytes!("../assets/tour/snap-light.webp"),
        ],
    },
    Step {
        title: "Math that answers itself",
        body: "Write a calculation and an equals sign. The answer appears in your own handwriting.",
        short: "Math",
        clip: [
            include_bytes!("../assets/tour/math-dark.webp"),
            include_bytes!("../assets/tour/math-light.webp"),
        ],
    },
    Step {
        title: "Turn writing into text",
        body: "Circle handwriting with the lasso and choose Convert to text. Review it, then replace the ink.",
        short: "Convert",
        clip: [
            include_bytes!("../assets/tour/convert-dark.webp"),
            include_bytes!("../assets/tour/convert-light.webp"),
        ],
    },
    Step {
        title: "Solve it step by step",
        body: "Select a problem and choose Solve. The solver reads it and works through it one step at a time.",
        short: "Solve",
        clip: [
            include_bytes!("../assets/tour/solve-dark.webp"),
            include_bytes!("../assets/tour/solve-light.webp"),
        ],
    },
    Step {
        title: "Find what you wrote",
        body: "Make handwriting searchable, then find it from the search bar like typed text.",
        short: "Find",
        clip: [
            include_bytes!("../assets/tour/search-dark.webp"),
            include_bytes!("../assets/tour/search-light.webp"),
        ],
    },
];

/// The closing step, after the features: it plays the first clip again.
const READY: Step = Step {
    title: "You are all set",
    body: "Try it all in the starter document, or start with a blank page. The tour stays in Help.",
    short: "Ready",
    clip: STEPS[0].clip,
};

/// The step at `index`, the closing one after the features.
fn step_at(index: usize) -> &'static Step {
    STEPS.get(index).unwrap_or(&READY)
}


/// The clips' shape: 960 × 600 pixels.
const CLIP_ASPECT: f32 = 1.6;

impl Tour {
    /// The tour from its first step.
    pub(super) fn start() -> Self {
        Self {
            step: 0,
            from: 0,
            forward: true,
            clip: None,
            opened: Instant::now(),
            stepped: Instant::now(),
            ripples: Ripples::default(),
        }
    }
}

// ------------------------------------------------------------ the player

/// A looping clip (animated WebP) played on the tour's stage. A worker
/// decodes it a few frames ahead of the clock, so only those frames are ever
/// in memory; the stage shows each frame when its time comes. Dropping the
/// clip ends the worker.
struct Clip {
    bytes: &'static [u8],
    pixels: rune::Pixels,
    frames: mpsc::Receiver<Frame>,
    started: Instant,
    /// When the frame on show ends, in seconds since `started`.
    shown_until: f32,
}
struct Frame {
    rgba: Vec<u8>,
    size: (u32, u32),
    seconds: f32,
}

impl Clip {
    fn play(bytes: &'static [u8]) -> Self {
        let (send, frames) = mpsc::sync_channel(3);
        std::thread::Builder::new()
            .name("folio-tour-clip".into())
            .spawn(move || {
                // Round and round until the stage stops listening.
                loop {
                    let Ok(decoder) = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes)) else {
                        return;
                    };
                    for frame in image::AnimationDecoder::into_frames(decoder) {
                        let Ok(frame) = frame else { return };
                        let (numer, denom) = frame.delay().numer_denom_ms();
                        let buffer = frame.into_buffer();
                        let next = Frame {
                            size: buffer.dimensions(),
                            rgba: buffer.into_raw(),
                            seconds: (numer as f32 / denom.max(1) as f32 / 1000.).max(0.01),
                        };
                        if send.send(next).is_err() {
                            return;
                        }
                    }
                }
            })
            .ok();
        Self {
            bytes,
            pixels: rune::Pixels::new(),
            frames,
            started: Instant::now(),
            shown_until: 0.,
        }
    }
    /// Shows the frame due `at` seconds in, taking frames from the worker as
    /// their times pass (or waiting for them, with `wait`, for snapshots).
    fn advance(&mut self, at: f32, wait: bool) {
        while self.shown_until <= at {
            let frame = if wait {
                self.frames.recv().ok()
            } else {
                self.frames.try_recv().ok()
            };
            let Some(frame) = frame else { break };
            self.shown_until += frame.seconds;
            self.pixels.set(frame.size.0, frame.size.1, frame.rgba);
        }
    }
}


// ------------------------------------------------------------ the dots

/// Points between the backdrop's dots.
const DOT_GAP: f32 = 14.;
/// A dot's radius at rest and fully lit, in points.
const DOT_RADIUS: (f32, f32) = (0.75, 1.7);
/// Simulation steps a second.
const RIPPLE_RATE: f32 = 110.;
/// How strongly each dot's water pulls on its neighbours' (the square of
/// the wave's speed in dots a step; it must stay below 0.5).
const RIPPLE_PULL: f32 = 0.06;
/// How much each dot's speed is pulled toward its neighbours' each step:
/// the water's viscosity. It smooths fine ripples away fastest, so waves
/// roll as broad, slow swells (it must stay below 0.3).
const RIPPLE_VISCOSITY: f32 = 0.16;
/// What a step leaves of the water's speed: how fast ripples die away.
const RIPPLE_KEEP: f32 = 0.985;
/// How deep a wave front must run to light a dot fully.
const RIPPLE_FULL: f32 = 0.5;

/// A solid part of the tour the water runs into, in window points: x, y,
/// width, height and corner radius.
type Wall = [f32; 5];

/// How far window point (`x`, `y`) lies outside `wall` (negative inside),
/// minding its rounded corners.
fn outside(wall: Wall, x: f32, y: f32) -> f32 {
    let [left, top, w, h, r] = wall;
    let r = r.min(w / 2.).min(h / 2.);
    let qx = (x - left - w / 2.).abs() - (w / 2. - r);
    let qy = (y - top - h / 2.).abs() - (h / 2. - r);
    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - r
}

/// The backdrop's dots as a sheet of water: a damped wave equation on the
/// grid, a cell for each dot. The pointer drags a wake through it, presses
/// drop splashes in it and the tour's buttons send out waves in their own
/// shape. The clip and the buttons stand in it as walls the waves come back
/// off, as do the window's edges. Each dot brightens and grows with the
/// wave front under it.
#[derive(Default)]
struct Ripples {
    cols: usize,
    rows: usize,
    /// Where the first dot sits in the window, so the grid is centred.
    origin: (f32, f32),
    height: Vec<f32>,
    speed: Vec<f32>,
    /// The cells under a wall: they hold still.
    solid: Vec<bool>,
    /// Steps owed to the clock.
    owed: f32,
    last: Option<Instant>,
    /// Still water: nothing to step until something stirs it.
    calm: bool,
    /// Where the pointer was last seen.
    pointer: Option<(f32, f32)>,
}

impl Ripples {
    /// Fits the grid to a window `w` × `h` points (new water when it changes).
    fn fit(&mut self, w: f32, h: f32) {
        let cols = (w / DOT_GAP).floor() as usize + 1;
        let rows = (h / DOT_GAP).floor() as usize + 1;
        self.origin = (
            (w - (cols - 1) as f32 * DOT_GAP) / 2.,
            (h - (rows - 1) as f32 * DOT_GAP) / 2.,
        );
        if (cols, rows) != (self.cols, self.rows) {
            (self.cols, self.rows) = (cols, rows);
            self.height = vec![0.; cols * rows];
            self.speed = vec![0.; cols * rows];
            self.solid = vec![false; cols * rows];
            self.calm = true;
        }
    }
    fn at(&self, i: usize, j: usize) -> (f32, f32) {
        (self.origin.0 + i as f32 * DOT_GAP, self.origin.1 + j as f32 * DOT_GAP)
    }
    /// Stands `walls` in the water, in place of the last ones.
    fn walls(&mut self, walls: &[Wall]) {
        for j in 0..self.rows {
            for i in 0..self.cols {
                let (x, y) = self.at(i, j);
                let k = j * self.cols + i;
                self.solid[k] = walls.iter().any(|&wall| outside(wall, x, y) < 0.);
                if self.solid[k] {
                    self.height[k] = 0.;
                    self.speed[k] = 0.;
                }
            }
        }
    }
    /// Pushes the water down by `amount` at each dot, as `weight` of its
    /// window point says (cells near (`x`, `y`) within `reach` dots).
    fn push_near(&mut self, (x, y): (f32, f32), reach: f32, amount: f32, weight: impl Fn(f32, f32) -> f32) {
        if self.cols < 3 || self.rows < 3 {
            return;
        }
        let (gx, gy) = ((x - self.origin.0) / DOT_GAP, (y - self.origin.1) / DOT_GAP);
        let reach = reach.ceil() as i64;
        let (cx, cy) = (gx.round() as i64, gy.round() as i64);
        for j in (cy - reach).max(1)..=(cy + reach).min(self.rows as i64 - 2) {
            for i in (cx - reach).max(1)..=(cx + reach).min(self.cols as i64 - 2) {
                let k = j as usize * self.cols + i as usize;
                if self.solid[k] {
                    continue;
                }
                let (px, py) = self.at(i as usize, j as usize);
                self.speed[k] -= amount * weight(px, py);
            }
        }
        self.calm = false;
    }
    /// A round splash `radius` dots across, from a press on the backdrop.
    fn splash(&mut self, x: f32, y: f32, strength: f32) {
        let radius = 1.1 * DOT_GAP;
        self.push_near((x, y), 3., 1.4 * strength, |px, py| {
            (-((px - x).powi(2) + (py - y).powi(2)) / (radius * radius)).exp()
        });
    }
    /// A wave in the shape of `wall` (a button just pressed), rising in a
    /// band just outside its edge and running out from there.
    fn splash_around(&mut self, wall: Wall, strength: f32) {
        let [x, y, w, h, _] = wall;
        let reach = (w.max(h) / 2. / DOT_GAP) + 3.;
        let (middle, band) = (0.7 * DOT_GAP, 0.55 * DOT_GAP);
        self.push_near((x + w / 2., y + h / 2.), reach, 1.1 * strength, |px, py| {
            let d = outside(wall, px, py);
            if d <= 0. { 0. } else { (-((d - middle) / band).powi(2)).exp() }
        });
    }
    /// The pointer moved to (`x`, `y`): it drags a thin wake along the way
    /// it came, as much as the distance it covered.
    fn pointer(&mut self, x: f32, y: f32) {
        if let Some((px, py)) = self.pointer {
            let distance = (x - px).hypot(y - py);
            // Coming back into the window is no stroke across it.
            if distance < 320. {
                let n = (distance / (DOT_GAP * 0.4)).ceil().max(1.);
                let each = 0.3 * distance / n / DOT_GAP;
                let radius = 0.6 * DOT_GAP;
                for k in 1..=n as usize {
                    let t = k as f32 / n;
                    let (cx, cy) = (px + (x - px) * t, py + (y - py) * t);
                    self.push_near((cx, cy), 2., each, |qx, qy| {
                        (-((qx - cx).powi(2) + (qy - cy).powi(2)) / (radius * radius)).exp()
                    });
                }
            }
        }
        self.pointer = Some((x, y));
    }
    /// Steps the water up to now.
    fn advance(&mut self) {
        let now = Instant::now();
        let dt = self.last.map_or(0., |last| now.duration_since(last).as_secs_f32()).min(0.1);
        self.last = Some(now);
        if self.calm {
            self.owed = 0.;
            return;
        }
        self.owed = (self.owed + dt * RIPPLE_RATE).min(9.);
        while self.owed >= 1. {
            self.owed -= 1.;
            self.step();
        }
    }
    fn step(&mut self) {
        let c = self.cols;
        // The nine-point Laplacian: ripples spread as circles, not diamonds,
        // on a square grid. Walls and edges hold at zero, so waves come back
        // off them.
        let laplacian = |f: &[f32], k: usize| {
            (4. * (f[k - 1] + f[k + 1] + f[k - c] + f[k + c])
                + f[k - c - 1]
                + f[k - c + 1]
                + f[k + c - 1]
                + f[k + c + 1]
                - 20. * f[k])
                / 6.
        };
        let (h, v) = (&self.height, &self.speed);
        let mut next = self.speed.clone();
        for j in 1..self.rows - 1 {
            for i in 1..c - 1 {
                let k = j * c + i;
                if self.solid[k] {
                    continue;
                }
                next[k] = (v[k] + RIPPLE_PULL * laplacian(h, k) + RIPPLE_VISCOSITY * laplacian(v, k)) * RIPPLE_KEEP;
            }
        }
        self.speed = next;
        let mut stirred = 0_f32;
        for (height, speed) in self.height.iter_mut().zip(&self.speed) {
            *height = (*height + speed) * 0.997;
            stirred = stirred.max(height.abs()).max(speed.abs());
        }
        if stirred < 2e-4 {
            self.height.fill(0.);
            self.speed.fill(0.);
            self.calm = true;
        }
    }
    /// Every dot outside the walls, at rest or lit by the wave front over
    /// it, as one shape in window points, fading out toward the window's
    /// edges. Two slow, broad waves cross the grid `t` seconds in, so even
    /// still water breathes a little.
    fn dots(&self, theme: Theme, w: f32, h: f32, t: f32) -> rune::Shape {
        let ink = rgb(theme.ink);
        let (rest, lit) = if theme.dark { (0.14, 0.6) } else { (0.15, 0.5) };
        let mut shape = rune::Shape::new();
        for j in 0..self.rows {
            for i in 0..self.cols {
                let k = j * self.cols + i;
                if self.solid.get(k).copied().unwrap_or(false) {
                    continue;
                }
                let (x, y) = self.at(i, j);
                let edge = (x.min(w - x).min(y).min(h - y) / 120.).clamp(0., 1.);
                let edge = edge * edge * (3. - 2. * edge);
                // The fronts light up (presses push the water down), the
                // swells between them stay dark: concentric rings.
                let water = self.height.get(k).copied().unwrap_or(0.);
                let glow = (-water / RIPPLE_FULL).clamp(0., 1.).powf(1.3);
                let breath = (0.5 + 0.5 * (x * 0.011 - y * 0.007 + t * 0.8).sin())
                    * (0.5 + 0.5 * (y * 0.009 + x * 0.004 - t * 0.55).sin());
                let alpha = rest * edge * (1. + 0.5 * breath) + (lit - rest * edge) * glow;
                if alpha < 0.01 {
                    continue;
                }
                let radius = DOT_RADIUS.0 + 0.15 * breath + (DOT_RADIUS.1 - DOT_RADIUS.0) * glow;
                shape = shape.stroke_with(
                    vec![[x, y], [x + 0.01, y]],
                    false,
                    radius * 2.,
                    rune::LineCap::Round,
                    ink.opacity(alpha).color(),
                );
            }
        }
        shape
    }
}

// ------------------------------------------------------------ the buttons

/// Back's and the main button's words at step `index`.
fn button_labels(index: usize) -> (&'static str, &'static str) {
    match index {
        0 => ("Back", "Show me"),
        i if i >= STEPS.len() => ("Start writing", "Open starter document"),
        _ => ("Back", "Next"),
    }
}

/// Where the two buttons sit at step `index`, in points about the middle of
/// their row, 8 apart: Back's centre, width and how far it shows, then the
/// main button's centre and width. On the first step Back waits, hidden,
/// where it will appear.
fn buttons_at(index: usize) -> ((f32, f32, f32), (f32, f32)) {
    let (back, on) = if index >= STEPS.len() { (132., 214.) } else { (76., 116.) };
    let total = back + 8. + on;
    let back_x = -total / 2. + back / 2.;
    if index == 0 {
        ((back_x, back, 0.), (0., on))
    } else {
        ((back_x, back, 1.), (total / 2. - on / 2., on))
    }
}

// ------------------------------------------------------------ motion

fn ease_out_expo(k: f32) -> f32 {
    if k >= 1. { 1. } else { 1. - 2_f32.powf(-10. * k.max(0.)) }
}
/// Overshoots a touch before settling, for things that pop.
fn ease_out_back(k: f32) -> f32 {
    let k = k.clamp(0., 1.) - 1.;
    1. + 2.70158 * k * k * k + 1.70158 * k * k
}
/// How far through an entrance `dur` seconds long that begins `delay`
/// seconds in the clock is at `t`.
fn progress(t: f32, delay: f32, dur: f32) -> f32 {
    ((t - delay) / dur).clamp(0., 1.)
}

/// The pose of something part way into its entrance (`p`, eased, from 0 to
/// 1): shifted by `(dx, dy)` and blurred by `blur` points at the start,
/// scaled up from `from`, fading in, plus a resting shift of `rest`. The
/// tour sets it every frame from its clocks, so its own motion is none.
fn pose(e: rune::El, p: f32, (dx, dy): (f32, f32), blur: f32, from: f32, rest: (f32, f32)) -> rune::El {
    let q = 1. - p;
    e.offset(dx * q + rest.0, dy * q + rest.1)
        .opacity(p.clamp(0., 1.))
        .blur((blur * q).max(0.))
        .scale(from + (1. - from) * p)
        .motion(rune::Spring::NONE)
}
/// `d` in that pose, its insides landing at once too.
fn arrive(d: Div, p: f32, shift: (f32, f32), blur: f32, from: f32, rest: (f32, f32)) -> Div {
    d.el(move |e| pose(e, p, shift, blur, from, rest).instant_changes())
}

impl NotesView {
    pub(super) fn open_tour(&mut self, window: &mut Window) {
        self.controller.finish();
        self.dismiss_popovers();
        self.help_open = false;
        self.tour = Some(Tour::start());
        self.focus.focus(window);
    }
    /// Closes the tour for good: it shows by itself only once.
    pub(super) fn close_tour(&mut self, window: &mut Window) {
        self.tour = None;
        if !self.controller.settings.tour_seen {
            self.controller.settings.tour_seen = true;
            self.controller.store_settings();
        }
        self.focus.focus(window);
    }
    /// Where the control `key` was laid out, as a wall with corners of
    /// `radius` (`None` for half its height: a pill).
    fn tour_wall(&self, key: &str, radius: Option<f32>) -> Option<Wall> {
        let rect = self.accessibility.control_bounds_for_key(key)?;
        let (w, h) = ((rect.x1 - rect.x0) as f32, (rect.y1 - rect.y0) as f32);
        Some([rect.x0 as f32, rect.y0 as f32, w, h, radius.unwrap_or(h / 2.)])
    }
    /// A wave in the shape of the control `key` (the button that moved the
    /// tour, or the one its key stands for).
    fn tour_splash(&mut self, key: &str) {
        let Some(wall) = self.tour_wall(key, if key.starts_with("tour-step-") { Some(7.) } else { None }) else {
            return;
        };
        if let Some(tour) = &mut self.tour {
            tour.ripples.splash_around(wall, 1.);
        }
    }
    /// Goes to step `next` (the closing step is `STEPS.len()`).
    fn tour_go(&mut self, next: usize) {
        if let Some(tour) = &mut self.tour {
            let next = next.min(STEPS.len());
            if next != tour.step {
                *tour = Tour {
                    step: next,
                    from: tour.step,
                    forward: next > tour.step,
                    clip: None,
                    opened: tour.opened,
                    stepped: Instant::now(),
                    ripples: std::mem::take(&mut tour.ripples),
                };
            }
        }
    }
    fn tour_step(&mut self, forward: bool) {
        let Some(tour) = &self.tour else {
            return;
        };
        let next = if forward { tour.step + 1 } else { tour.step.saturating_sub(1) };
        self.tour_splash(if forward { "tour-next" } else { "tour-back" });
        self.tour_go(next);
    }
    /// The tour's keys: ← and → step, Enter goes on.
    pub(super) fn tour_key(&mut self, key: &str, window: &mut Window) -> bool {
        let Some(tour) = &self.tour else {
            return false;
        };
        match key {
            "right" | "enter" if tour.step >= STEPS.len() => self.close_tour(window),
            "right" | "enter" => self.tour_step(true),
            "left" => self.tour_step(false),
            _ => return false,
        }
        true
    }
    /// The tour, filling the window: one centred column of the step's clip,
    /// title, words and buttons, the steps named along the foot, over the
    /// rippling dots. The column's rows keep their places from step to
    /// step; the clip and words arrive afresh, and the buttons make room for
    /// each other.
    pub(super) fn tour_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let reduce = self.controller.settings.reduce_motion;
        let size = window.viewport_size();
        let (w, h) = (f32::from(size.width), f32::from(size.height));
        let Some(index) = self.tour.as_ref().map(|tour| tour.step) else {
            return div();
        };
        let last = index >= STEPS.len();
        // The clip and the buttons stand in the water.
        let walls: Vec<Wall> = [
            self.tour_wall("tour-stage", Some(14.)),
            self.tour_wall("tour-next", None),
            if index > 0 { self.tour_wall("tour-back", None) } else { None },
        ]
        .into_iter()
        .flatten()
        .collect();
        let tour = self.tour.as_mut().expect("the tour is open");
        let forward = tour.forward;
        let step = step_at(index);
        let bytes = step.clip[usize::from(!theme.dark)];
        if tour.clip.as_ref().is_none_or(|clip| !std::ptr::eq(clip.bytes, bytes)) {
            tour.clip = Some(Clip::play(bytes));
        }
        // Debug builds can pin the clocks for headless snapshots, whose
        // frames run faster than real time.
        #[cfg(debug_assertions)]
        let pinned = std::env::var("FOLIO_TOUR_AT").ok().and_then(|at| at.parse::<f32>().ok());
        #[cfg(not(debug_assertions))]
        let pinned: Option<f32> = None;
        let (opened, stepped) = (tour.opened, tour.stepped);
        let clip = tour.clip.as_mut().expect("just made");
        let at = pinned.unwrap_or_else(|| clip.started.elapsed().as_secs_f32());
        clip.advance(at, pinned.is_some());
        let pixels = clip.pixels.clone();
        // The entrances' clocks; Reduce Motion shows everything in place
        // and keeps the water and the clip still.
        let clock = |since: Instant| {
            if reduce { 99. } else { pinned.unwrap_or_else(|| since.elapsed().as_secs_f32()) }
        };
        let (t_open, t_step) = (clock(opened), clock(stepped));
        let ambient = if reduce { 0. } else { clock(opened) };
        tour.ripples.fit(w, h);
        tour.ripples.walls(&walls);
        if !reduce {
            tour.ripples.advance();
        }
        let dots = tour.ripples.dots(theme, w, h, ambient);
        let (from, back_label_from, on_label_from) = {
            let from = tour.from;
            let (back, on) = button_labels(from);
            (from, back, on)
        };

        let bar_h = 40.;
        // The clip, words and buttons sit in the middle of the window, clear
        // of the top bar and the step names.
        let clip_w = (w - 48.).min((h - 410.) * CLIP_ASPECT).clamp(280., 720.);
        let side = if forward { 1. } else { -1. };
        let line = rgb(theme.ink).opacity(if theme.dark { 0.12 } else { 0.1 });

        // The water: every dot in one shape, stirred by the pointer moving
        // over the tour and by presses where no control is.
        let water = div()
            .id("tour-dots")
            .absolute()
            .inset_0()
            .el(move |e| {
                e.shape(dots)
                    .motion(rune::Spring::NONE)
                    .instant_changes()
                    .on_pointer_move(|s: &mut AppState, p: rune::PointerPress| {
                        if let Some(tour) = &mut s.view.get_mut().tour {
                            tour.ripples.pointer(p.x, p.y);
                        }
                    })
                    .drag_threshold(0.)
                    .on_drag(|s: &mut AppState, drag: &mut rune::Drag, _: &mut rune::Tx| {
                        if drag.phase == rune::Phase::Down
                            && let Some(tour) = &mut s.view.get_mut().tour
                        {
                            tour.ripples.splash(drag.x, drag.y, 1.);
                        }
                    })
            });

        let skip = (!last).then(|| {
            self.button("tour-skip", "Skip", false, cx, |this, w, _| this.close_tour(w))
                .h(px(30.))
                .px(px(12.))
                .rounded_full()
                .text_color(rgb(theme.muted))
        });
        let bar = arrive(
            div()
                .h(px(bar_h))
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(super::titlebar::drag_region())
                .children(skip)
                .child(self.window_controls(window, cx)),
            ease_out_expo(progress(t_open, 0.05, 0.8)),
            (0., -10.),
            0.,
            1.,
            (0., 0.),
        );

        let count = if last {
            "Done".to_string()
        } else {
            format!("{} / {}", index + 1, STEPS.len())
        };
        let count = arrive(
            div()
                .id(format!("tour-count-{index}"))
                .h(px(18.))
                .mb(px(12.))
                .text_xs()
                .text_color(rgb(theme.muted))
                .child(count),
            ease_out_expo(progress(t_step, 0.03, 0.7)),
            (0., 8.),
            4.,
            1.,
            (0., 0.),
        );
        // The clip on paper, its hairline edge drawn over it (the clip would
        // cover a border of its own) and, on a white window, a drop shadow.
        let stage = div()
            .id(format!("tour-clip-{index}"))
            .relative()
            .w(px(clip_w))
            .h(px(clip_w / CLIP_ASPECT))
            .flex_shrink_0()
            .rounded(px(14.))
            .when(!theme.dark, |stage| {
                stage.el(|e| {
                    e.box_shadow(rune::Shadow {
                        x: 0.,
                        y: 1.,
                        blur: 3.,
                        spread: 0.,
                        color: rgba(0x00000014).color(),
                        inset: false,
                    })
                    .box_shadow(rune::Shadow {
                        x: 0.,
                        y: 16.,
                        blur: 44.,
                        spread: -10.,
                        color: rgba(0x00000038).color(),
                        inset: false,
                    })
                })
            })
            .child(
                div()
                    .size_full()
                    .rounded(px(14.))
                    .overflow_hidden()
                    .bg(rgb(theme.canvas.paper))
                    .el(move |e| e.pixels_fit(&pixels, rune::SurfaceFit::Fit)),
            )
            .child(div().absolute().inset_0().rounded(px(14.)).border_1().border_color(line));
        // It slides in from the side the tour moved to, out of a blur, and
        // floats a little; its laid-out place is the water's wall.
        let stage = arrive(
            div().track_bounds("tour-stage").child(stage),
            ease_out_expo(progress(t_step, 0., 1.)),
            (side * 60., 0.),
            10.,
            0.94,
            (0., 3. * (ambient * 1.1).sin()),
        );
        // The title rises word by word out of a blur, the words after it.
        let words: Vec<&str> = step.title.split(' ').collect();
        let mut title = div().mt(px(26.)).h(px(34.)).flex().justify_center();
        for (i, word) in words.iter().enumerate() {
            title = title.child(arrive(
                div()
                    .id(format!("tour-word-{index}-{i}"))
                    .when(i + 1 < words.len(), |w| w.mr(px(7.)))
                    .text_size(px(26.))
                    .line_height(px(34.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(*word),
                ease_out_expo(progress(t_step, 0.08 + i as f32 * 0.06, 0.9)),
                (0., 22.),
                8.,
                1.,
                (0., 0.),
            ));
        }
        // Two lines are kept for the words, so nothing below them moves.
        let body = arrive(
            div()
                .id(format!("tour-body-{index}"))
                .mt(px(8.))
                .h(px(44.))
                .w(px(clip_w.min(520.)))
                .text_size(px(14.))
                .line_height(px(22.))
                .text_color(rgb(theme.muted))
                .text_center()
                .child(step.body),
            ease_out_expo(progress(t_step, 0.2 + words.len() as f32 * 0.05, 0.9)),
            (0., 14.),
            4.,
            1.,
            (0., 0.),
        );

        // The buttons, each a set width placed about the middle, so the
        // morph between steps is the tour's to draw: the first step's sits
        // alone in the middle; from the second on it glides aside and
        // resizes as Back fades and grows in beside it, and their words
        // cross-fade inside them.
        let morph = ease_out_expo(progress(t_step, 0., 0.75));
        let (was, now) = (buttons_at(from), buttons_at(index));
        let mix = |a: f32, b: f32| a + (b - a) * morph;
        let (back_x, back_w, back_shown) = (mix(was.0.0, now.0.0), mix(was.0.1, now.0.1), mix(was.0.2, now.0.2));
        let (on_x, on_w) = (mix(was.1.0, now.1.0), mix(was.1.1, now.1.1));
        let (back_label, on_label) = button_labels(index);
        let fade_out = 1. - progress(t_step, 0., 0.16);
        let fade_in = progress(t_step, 0.08, 0.24);
        let label = |text: &'static str, colour: u32, arrow: bool, alpha: f32| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .child(text)
                .when(arrow, |l| l.child(icon(Icon::Forward, colour).size(px(14.))))
                .el(move |e| e.opacity(alpha).motion(rune::Spring::NONE))
        };
        let labels = |was: &'static str, now: &'static str, colour: u32, arrow: bool| {
            let mut stack = div().size_full().relative();
            if was != now && fade_out > 0. {
                stack = stack.child(label(was, colour, arrow, fade_out));
            }
            stack.child(label(now, colour, arrow, if was == now { 1. } else { fade_in }))
        };
        const ROW: f32 = 480.;
        let on = self
            .control(
                "tour-next",
                on_label,
                labels(on_label_from, on_label, theme.primary_foreground, true).into_any_element(),
                false,
                cx,
                move |this, w, cx| {
                    if this.tour.as_ref().is_some_and(|tour| tour.step >= STEPS.len()) {
                        this.close_tour(w);
                        this.controller.create_starter_notebook();
                        this.show_editor();
                    } else {
                        this.tour_step(true);
                    }
                    cx.notify();
                },
            )
            .absolute()
            .top_0()
            .left(px(ROW / 2. + on_x - on_w / 2.))
            .w(px(on_w))
            .h(px(34.))
            .p_0()
            .rounded_full()
            .bg(rgb(theme.accent))
            .text_color(rgb(theme.primary_foreground))
            .font_weight(FontWeight::SEMIBOLD)
            .el(|e| e.motion(rune::Spring::NONE));
        let back = (back_shown > 0.01).then(|| {
            self.control(
                "tour-back",
                back_label,
                labels(back_label_from, back_label, theme.ink, false).into_any_element(),
                false,
                cx,
                |this, w, cx| {
                    if this.tour.as_ref().is_some_and(|tour| tour.step >= STEPS.len()) {
                        this.close_tour(w);
                    } else {
                        this.tour_step(false);
                    }
                    cx.notify();
                },
            )
            .absolute()
            .top_0()
            .left(px(ROW / 2. + back_x - back_w / 2.))
            .w(px(back_w))
            .h(px(34.))
            .p_0()
            .rounded_full()
            .border_1()
            .border_color(line)
            .el(move |e| {
                e.opacity(back_shown)
                    .scale(0.85 + 0.15 * back_shown)
                    .motion(rune::Spring::NONE)
            })
        });
        let open = ease_out_expo(progress(t_open, 0.35, 0.8));
        let actions = div()
            .id("tour-actions")
            .relative()
            .mt(px(22.))
            .w(px(ROW))
            .h(px(34.))
            .children(back)
            .child(on)
            .el(move |e| pose(e, open, (0., 12.), 0., 1., (0., 0.)).instant_changes());

        // The steps named along the foot, the current one underlined; each
        // takes you there. They pop in one after another as the tour opens.
        let mut names = div().flex().items_center().gap_2();
        for i in 0..=STEPS.len() {
            let current = i == index;
            let colour = if current {
                rgb(theme.ink)
            } else if i < index {
                rgb(theme.muted)
            } else {
                rgb(theme.muted).opacity(0.55)
            };
            let content = div()
                .relative()
                .pb(px(5.))
                .text_xs()
                .text_color(colour)
                .child(step_at(i).short)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(1.5))
                        .rounded_full()
                        .bg(rgb(theme.ink))
                        .opacity(if current { 1. } else { 0. })
                        .el(|e| e.motion(crate::motion::GLIDE).animated_changes()),
                );
            let name = format!("tour-step-{i}");
            let chip = self
                .control(name.clone(), step_at(i).short, content.into_any_element(), false, cx, move |this, _, cx| {
                    this.tour_splash(&name);
                    this.tour_go(i);
                    cx.notify();
                })
                .h(px(30.))
                .px(px(8.))
                .pt(px(5.))
                .rounded(px(7.));
            let pop = ease_out_back(progress(t_open, 0.3 + i as f32 * 0.05, 0.6));
            names = names.child(div().child(chip).el(move |e| pose(e, pop, (0., 16.), 0., 0.6, (0., 0.))));
        }

        let column = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(px(24.))
            // As far up from the foot as the top bar comes down, so the
            // group sits in the middle of the window.
            .pb(px(bar_h))
            .child(count)
            .child(stage)
            .child(title)
            .child(body)
            .child(actions);
        let names = div().absolute().left_0().right_0().bottom(px(20.)).flex().justify_center().child(names);

        div()
            .id("tour-screen")
            .absolute()
            .inset_0()
            .occlude()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .el(|e| {
                e.transition(rune::Transition::scale(1.02).springs(rune::SpringPair::new(
                    crate::motion::SETTLE,
                    crate::motion::SETTLE,
                )))
                .animated_changes()
            })
            .child(water)
            .child(div().absolute().inset_0().flex().flex_col().child(bar).child(column))
            .child(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn every_tour_clip_plays_on_the_stage() {
        for (step, clip) in STEPS.iter().flat_map(|step| step.clip.map(|clip| (step, clip))) {
            let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(clip))
                .unwrap_or_else(|e| panic!("{}: {e}", step.title));
            let frames: Vec<_> = image::AnimationDecoder::into_frames(decoder)
                .take(3)
                .collect::<Result<_, _>>()
                .unwrap_or_else(|e| panic!("{}: {e}", step.title));
            assert_eq!(frames.len(), 3, "{} animates", step.title);
            assert_eq!(frames[0].buffer().dimensions(), (960, 600), "1.6 times as wide as tall");
        }
    }
    #[::core::prelude::v1::test]
    fn a_splash_spreads_as_a_ring_comes_back_off_walls_and_dies_away() {
        let mut water = Ripples::default();
        water.fit(960., 600.);
        // A wall a little right of the splash.
        let wall: Wall = [480. + 8. * DOT_GAP, 0., 4. * DOT_GAP, 600., 0.];
        water.walls(&[wall]);
        water.splash(480., 300., 1.);
        let row = ((300. - water.origin.1) / DOT_GAP).round() as usize;
        let at = |water: &Ripples, dots: f32| {
            let col = ((480. + dots * DOT_GAP - water.origin.0) / DOT_GAP).round() as usize;
            water.height[row * water.cols + col]
        };
        for _ in 0..18 {
            water.step();
        }
        // The front has left the middle for a ring.
        let ring = (0..8).map(|d| at(&water, d as f32).abs()).enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        assert!(ring.0 >= 2, "the front travels out: {ring:?}");
        assert!(at(&water, 0.).abs() < ring.1, "and leaves the middle behind");
        for _ in 0..40 {
            water.step();
            assert_eq!(at(&water, 9.), 0., "nothing moves inside the wall");
        }
        for _ in 0..2000 {
            water.step();
        }
        assert!(water.calm, "the water settles");
    }
}
