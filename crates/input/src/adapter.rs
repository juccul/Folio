//! Adapters from platform tablet reports to Folio's pen samples and pad actions.
//!
//! Platforms disagree about the edges of a pen's range: tablets that report no pressure
//! (pucks, some screen pens), no tilt, a light resting force before the tip is pressed, or
//! a maximum the hand rarely reaches. [`ForceAngle`] maps every report to a force between
//! 0 and 1 and a tilt in degrees that ink width and shading can rely on. [`PadInput`] turns
//! tablet pad controls into navigation steps.

/// A pen report as the platform gave it, already scaled to 0..1 pressure and degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawPen {
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub has_pressure: bool,
    pub has_tilt: bool,
    /// Whether the tip touches the surface.
    pub contact: bool,
}

/// Maps raw pen force and angle to the values ink uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceAngle {
    /// Force below this, while touching, counts as the lightest stroke (a tip's resting force).
    pub floor: f32,
    /// Force at or above this counts as full pressure.
    pub ceiling: f32,
    /// The force of a device that reports none, and of the mouse.
    pub constant: f32,
    /// The lightest force a touching tip produces, so the start of a stroke is never invisible.
    pub minimum: f32,
    /// Largest tilt passed on, in degrees from upright.
    pub max_tilt: f32,
}

impl Default for ForceAngle {
    fn default() -> Self {
        Self {
            floor: 0.02,
            ceiling: 0.98,
            constant: 0.6,
            minimum: 0.05,
            max_tilt: 80.,
        }
    }
}

impl ForceAngle {
    /// The force of a report: 0 off the surface, `constant` for a device without pressure,
    /// otherwise the raw force stretched from `floor..ceiling` to `minimum..1`.
    pub fn force(&self, pen: RawPen) -> f32 {
        if !pen.contact {
            return 0.;
        }
        if !pen.has_pressure || !pen.pressure.is_finite() {
            return self.constant;
        }
        let span = (self.ceiling - self.floor).max(f32::EPSILON);
        let t = ((pen.pressure - self.floor) / span).clamp(0., 1.);
        self.minimum + (1. - self.minimum) * t
    }

    /// The tilt of a report in degrees, `(0, 0)` upright or for a device without tilt. The
    /// combined lean is limited to `max_tilt`, keeping its direction.
    pub fn tilt(&self, pen: RawPen) -> (f32, f32) {
        if !pen.has_tilt || !pen.tilt_x.is_finite() || !pen.tilt_y.is_finite() {
            return (0., 0.);
        }
        let (x, y) = (pen.tilt_x.clamp(-90., 90.), pen.tilt_y.clamp(-90., 90.));
        let lean = x.hypot(y);
        if lean <= self.max_tilt || lean == 0. {
            (x, y)
        } else {
            (x * self.max_tilt / lean, y * self.max_tilt / lean)
        }
    }

    /// The pen's altitude above the surface (radians, π/2 upright) and the azimuth of its
    /// lean (radians clockwise from the positive x axis), from tilt in degrees.
    pub fn altitude_azimuth(tilt_x: f32, tilt_y: f32) -> (f32, f32) {
        let (tx, ty) = (tilt_x.to_radians().tan(), tilt_y.to_radians().tan());
        let lean = tx.hypot(ty);
        if lean == 0. {
            return (std::f32::consts::FRAC_PI_2, 0.);
        }
        (
            lean.recip().atan(),
            ty.atan2(tx).rem_euclid(std::f32::consts::TAU),
        )
    }
}

/// A pad control's effect on the view.
#[derive(Clone, Debug, PartialEq)]
pub enum PadAction {
    /// The action configured for an express key, by its setting slug.
    Button(String),
    /// Zoom by this factor around the canvas centre.
    Zoom(f32),
    /// Scroll the canvas by this many points (positive moves the content up).
    Scroll(f32),
}

/// Turns pad buttons, rings and strips into [`PadAction`]s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadInput {
    /// Zoom per degree a ring turns, as an exponent: a full turn zooms by `e^(360 * this)`.
    pub ring_zoom: f32,
    /// Points scrolled for a whole strip's length.
    pub strip_scroll: f32,
}

impl Default for PadInput {
    fn default() -> Self {
        Self {
            ring_zoom: 0.01,
            strip_scroll: 800.,
        }
    }
}

impl PadInput {
    /// The action of express key `button` pressed, from the configured slugs.
    pub fn button(&self, button: u32, pressed: bool, slugs: &[String]) -> Option<PadAction> {
        let slug = slugs.get(button as usize).filter(|s| !s.is_empty())?;
        pressed.then(|| PadAction::Button(slug.clone()))
    }

    /// A ring turned by `degrees` (clockwise positive) zooms in.
    pub fn ring(&self, degrees: f32) -> Option<PadAction> {
        (degrees.is_finite() && degrees != 0.)
            .then(|| PadAction::Zoom((degrees * self.ring_zoom).exp()))
    }

    /// A strip moved by `fraction` of its length scrolls.
    pub fn strip(&self, fraction: f32) -> Option<PadAction> {
        (fraction.is_finite() && fraction != 0.)
            .then(|| PadAction::Scroll(fraction * self.strip_scroll))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen(pressure: f32, contact: bool) -> RawPen {
        RawPen {
            pressure,
            has_pressure: true,
            contact,
            ..Default::default()
        }
    }

    #[test]
    fn force_covers_the_whole_range_and_devices_without_pressure() {
        let adapter = ForceAngle::default();
        assert_eq!(adapter.force(pen(0.7, false)), 0.);
        // A tip resting below the floor still draws its lightest line.
        assert_eq!(adapter.force(pen(0.01, true)), adapter.minimum);
        assert_eq!(adapter.force(pen(0.99, true)), 1.);
        let mid = adapter.force(pen(0.5, true));
        assert!(mid > 0.45 && mid < 0.55, "{mid}");
        assert!(adapter.force(pen(0.3, true)) < adapter.force(pen(0.6, true)));
        let puck = RawPen {
            contact: true,
            ..Default::default()
        };
        assert_eq!(adapter.force(puck), adapter.constant);
        assert_eq!(adapter.force(pen(f32::NAN, true)), adapter.constant);
    }

    #[test]
    fn tilt_is_limited_in_its_direction_and_absent_without_a_sensor() {
        let adapter = ForceAngle::default();
        let tilted = RawPen {
            tilt_x: 30.,
            tilt_y: -40.,
            has_tilt: true,
            ..Default::default()
        };
        assert_eq!(adapter.tilt(tilted), (30., -40.));
        let steep = RawPen {
            tilt_x: 90.,
            tilt_y: 90.,
            has_tilt: true,
            ..Default::default()
        };
        let (x, y) = adapter.tilt(steep);
        assert!((x.hypot(y) - adapter.max_tilt).abs() < 1e-3 && (x - y).abs() < 1e-4);
        assert_eq!(
            adapter.tilt(RawPen {
                tilt_x: 20.,
                ..Default::default()
            }),
            (0., 0.)
        );

        let (altitude, _) = ForceAngle::altitude_azimuth(0., 0.);
        assert!((altitude - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let (altitude, azimuth) = ForceAngle::altitude_azimuth(0., 45.);
        assert!((altitude - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        assert!((azimuth - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn pad_controls_map_to_actions() {
        let pad = PadInput::default();
        let slugs = vec!["undo".to_string(), String::new()];
        assert_eq!(
            pad.button(0, true, &slugs),
            Some(PadAction::Button("undo".into()))
        );
        assert_eq!(pad.button(0, false, &slugs), None);
        assert_eq!(pad.button(1, true, &slugs), None);
        assert_eq!(pad.button(7, true, &slugs), None);
        let Some(PadAction::Zoom(zoom)) = pad.ring(30.) else {
            panic!()
        };
        assert!(zoom > 1.);
        let Some(PadAction::Zoom(zoom)) = pad.ring(-30.) else {
            panic!()
        };
        assert!(zoom < 1.);
        assert_eq!(pad.ring(0.), None);
        assert_eq!(pad.strip(0.25), Some(PadAction::Scroll(200.)));
    }
}
