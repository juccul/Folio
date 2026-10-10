//! Shared HSV color box edits a draft; Save/Cancel keep existing semantics.
use super::*;
use folio_app::appearance::ThemeColor;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Hsv {
    hue: f32,
    saturation: f32,
    value: f32,
}

impl Hsv {
    fn from_color(color: ThemeColor, hue: f32) -> Self {
        let rgb = color.rgb();
        let [r, g, b] = [16, 8, 0].map(|shift| ((rgb >> shift) & 255) as f32 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let hue = if delta == 0. {
            hue
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.) / 6.
        } else if max == g {
            ((b - r) / delta + 2.) / 6.
        } else {
            ((r - g) / delta + 4.) / 6.
        };
        Self {
            hue,
            saturation: if max == 0. { 0. } else { delta / max },
            value: max,
        }
    }

    fn color(self, alpha: u8) -> ThemeColor {
        let h = self.hue.rem_euclid(1.) * 6.;
        let chroma = self.value * self.saturation;
        let x = chroma * (1. - (h.rem_euclid(2.) - 1.).abs());
        let m = self.value - chroma;
        let channels = match h as u8 {
            0 => [chroma, x, 0.],
            1 => [x, chroma, 0.],
            2 => [0., chroma, x],
            3 => [0., x, chroma],
            4 => [x, 0., chroma],
            _ => [chroma, 0., x],
        };
        let [r, g, b] = channels.map(|c| ((c + m).clamp(0., 1.) * 255.).round() as u32);
        ThemeColor((r << 24) | (g << 16) | (b << 8) | alpha as u32)
    }
}

pub(super) struct PickerState {
    field: EntityId,
    color: ThemeColor,
    hsv: Hsv,
    textures: Rc<RefCell<[Option<PickerTexture>; 2]>>,
}

#[derive(Clone, Copy, PartialEq)]
struct TextureKey {
    hue: Option<f32>,
    width: u32,
    height: u32,
}

struct PickerTexture {
    key: TextureKey,
    image: Arc<RenderImage>,
}

fn picker_pixels(key: TextureKey) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(key.width as usize * key.height as usize * 4);
    for row in 0..key.height {
        for col in 0..key.width {
            let x = (col as f32 + 0.5) / key.width as f32;
            let hsv = match key.hue {
                Some(hue) => Hsv {
                    hue,
                    saturation: x,
                    value: 1. - (row as f32 + 0.5) / key.height as f32,
                },
                None => Hsv {
                    hue: x,
                    saturation: 1.,
                    value: 1.,
                },
            };
            let color = hsv.color(255).0;
            // RenderImage accepts straight BGRA bytes without gradient color
            // space interpolation, so its pixels match the selected RGB.
            pixels.extend_from_slice(&[
                (color >> 8) as u8,
                (color >> 16) as u8,
                (color >> 24) as u8,
                255,
            ]);
        }
    }
    pixels
}

fn paint_picker_texture(
    bounds: Bounds<Pixels>,
    hue: Option<f32>,
    texture: &mut Option<PickerTexture>,
    window: &mut Window,
) {
    let scale = window.scale_factor();
    let key = TextureKey {
        hue,
        width: (f32::from(bounds.size.width) * scale).ceil().max(1.) as u32,
        height: (f32::from(bounds.size.height) * scale).ceil().max(1.) as u32,
    };
    if texture.as_ref().is_none_or(|texture| texture.key != key) {
        if let Some(previous) = texture.take() {
            let _ = window.drop_image(previous.image);
        }
        let buffer = image::RgbaImage::from_raw(key.width, key.height, picker_pixels(key)).unwrap();
        *texture = Some(PickerTexture {
            key,
            image: Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
                buffer
            )])),
        });
    }
    let image = texture.as_ref().unwrap().image.clone();
    let _ = window.paint_image(bounds, Corners::default(), image, 0, false);
}

fn marker(bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(quad(
        bounds,
        px(8.),
        rgba(0),
        px(3.),
        rgb(0xffffff),
        BorderStyle::Solid,
    ));
    window.paint_quad(quad(
        bounds,
        px(8.),
        rgba(0),
        px(1.),
        rgb(0x171717),
        BorderStyle::Solid,
    ));
}

pub(super) fn checkerboard(bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(fill(bounds, rgb(0xffffff)));
    let tile = 8.;
    for row in 0..(f32::from(bounds.size.height) / tile).ceil() as usize {
        for col in 0..(f32::from(bounds.size.width) / tile).ceil() as usize {
            if (row + col) % 2 == 0 {
                let origin = bounds.origin + point(px(col as f32 * tile), px(row as f32 * tile));
                let size = size(
                    (bounds.right() - origin.x).min(px(tile)),
                    (bounds.bottom() - origin.y).min(px(tile)),
                );
                window.paint_quad(fill(Bounds::new(origin, size), rgb(0xd2d2d2)));
            }
        }
    }
}

impl NotesView {
    pub(super) fn clear_color_picker(&mut self, window: &mut Window) {
        self.color_drag = None;
        if let Some(state) = self.color_picker.take() {
            for texture in state.textures.borrow_mut().iter_mut() {
                if let Some(texture) = texture.take() {
                    let _ = window.drop_image(texture.image);
                }
            }
        }
    }

    fn picker_hsv(&mut self, field: &Entity<Field>, color: ThemeColor) -> Hsv {
        match &mut self.color_picker {
            Some(state) if state.field == field.entity_id() => {
                if state.color != color {
                    state.hsv = Hsv::from_color(color, state.hsv.hue);
                    state.color = color;
                }
                state.hsv
            }
            _ => {
                let hsv = Hsv::from_color(color, 0.);
                self.color_picker = Some(PickerState {
                    field: field.entity_id(),
                    color,
                    hsv,
                    textures: Rc::new(RefCell::new([None, None])),
                });
                hsv
            }
        }
    }

    fn set_picker_color(
        &mut self,
        field: &Entity<Field>,
        hsv: Hsv,
        alpha: u8,
        cx: &mut Context<Self>,
    ) {
        let color = hsv.color(alpha);
        match &mut self.color_picker {
            Some(state) if state.field == field.entity_id() => {
                state.color = color;
                state.hsv = hsv;
            }
            _ => {
                self.color_picker = Some(PickerState {
                    field: field.entity_id(),
                    color,
                    hsv,
                    textures: Rc::new(RefCell::new([None, None])),
                })
            }
        }
        field.update(cx, |f, cx| f.set_content(color.hex(), cx));
        // A hue change at white/black has no RGB change but must repaint the box.
        cx.notify();
    }

    pub(super) fn color_selector(
        &mut self,
        field: Entity<Field>,
        opacity: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let color = ThemeColor::parse(&field.read(cx).content)
            .unwrap_or(ThemeColor::opaque(theme.canvas.paper));
        let hsv = self.picker_hsv(&field, color);
        let textures = self.color_picker.as_ref().unwrap().textures.clone();
        let mut picker = div().flex().flex_col().gap_3();
        for channel in 0..if opacity { 3 } else { 2 } {
            let bounds = Rc::new(Cell::new(Bounds::default()));
            let measured = bounds.clone();
            let entity = cx.entity();
            let drag_field = field.clone();
            let textures = textures.clone();
            let track = canvas(
                move |b, _, _| measured.set(b),
                move |b, _, window, _| {
                    match channel {
                        0 => {
                            paint_picker_texture(
                                b,
                                Some(hsv.hue),
                                &mut textures.borrow_mut()[0],
                                window,
                            );
                            let center = b.origin
                                + point(
                                    b.size.width * hsv.saturation,
                                    b.size.height * (1. - hsv.value),
                                );
                            marker(
                                Bounds::new(center - point(px(7.), px(7.)), size(px(14.), px(14.))),
                                window,
                            );
                        }
                        1 => {
                            paint_picker_texture(b, None, &mut textures.borrow_mut()[1], window);
                            marker(
                                Bounds::new(
                                    point(
                                        b.origin.x + (b.size.width - px(8.)) * hsv.hue,
                                        b.origin.y + px(1.),
                                    ),
                                    size(px(8.), b.size.height - px(2.)),
                                ),
                                window,
                            );
                        }
                        _ => {
                            checkerboard(b, window);
                            window.paint_quad(fill(
                                b,
                                linear_gradient(
                                    90.,
                                    linear_color_stop(rgba(color.0 & !255), 0.),
                                    linear_color_stop(rgba(color.0 | 255), 1.),
                                ),
                            ));
                            marker(
                                Bounds::new(
                                    point(
                                        b.origin.x + (b.size.width - px(8.)) * color.alpha(),
                                        b.origin.y + px(1.),
                                    ),
                                    size(px(8.), b.size.height - px(2.)),
                                ),
                                window,
                            );
                        }
                    }
                    let move_entity = entity.clone();
                    let move_field = drag_field.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase.capture() {
                            move_entity.update(cx, |this, cx| {
                                if this.color_drag == Some((move_field.entity_id(), channel)) {
                                    if event.pressed_button == Some(MouseButton::Left) {
                                        this.pick_color(
                                            &move_field,
                                            channel,
                                            event.position,
                                            b,
                                            opacity,
                                            cx,
                                        );
                                    } else {
                                        this.color_drag = None;
                                    }
                                    cx.stop_propagation();
                                }
                            });
                        }
                    });
                    let up_entity = entity.clone();
                    let up_field = drag_field.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase.capture() && event.button == MouseButton::Left {
                            up_entity.update(cx, |this, cx| {
                                if this.color_drag == Some((up_field.entity_id(), channel)) {
                                    this.pick_color(
                                        &up_field,
                                        channel,
                                        event.position,
                                        b,
                                        opacity,
                                        cx,
                                    );
                                    this.color_drag = None;
                                    cx.stop_propagation();
                                }
                            });
                        }
                    });
                },
            )
            .w_full()
            .h(px(if channel == 0 { 180. } else { 26. }));
            let (id, label) = match channel {
                0 => (
                    "picker-saturation-value",
                    format!(
                        "Saturation and brightness: {:.0}%, {:.0}%. Arrow keys adjust; Shift adjusts by 10%",
                        hsv.saturation * 100.,
                        hsv.value * 100.
                    ),
                ),
                1 => (
                    "picker-hue",
                    format!(
                        "Hue: {:.0} degrees. Arrow keys adjust; Shift adjusts by 10 degrees",
                        hsv.hue * 360.
                    ),
                ),
                _ => (
                    "picker-opacity",
                    format!(
                        "Opacity: {:.0}%. Arrow keys adjust; Shift adjusts by 10%",
                        color.alpha() * 100.
                    ),
                ),
            };
            let down_field = field.clone();
            let key_field = field.clone();
            let control = self
                .control(id, label, track.into_any_element(), false, cx, |_, _, _| {})
                .w_full()
                .min_w_0()
                .p_0()
                .overflow_hidden()
                .rounded(px(4.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.accessibility.focus_control(id, window);
                        this.color_drag = Some((down_field.entity_id(), channel));
                        this.pick_color(
                            &down_field,
                            channel,
                            event.position,
                            bounds.get(),
                            opacity,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" {
                        this.submit_modal(window, cx);
                        cx.stop_propagation();
                        return;
                    }
                    let current = ThemeColor::parse(&key_field.read(cx).content).unwrap_or(color);
                    let mut hsv = this.picker_hsv(&key_field, current);
                    let mut alpha = if opacity { current.alpha() } else { 1. };
                    let step = if event.keystroke.modifiers.shift {
                        10.
                    } else {
                        1.
                    };
                    let key = event.keystroke.key.as_str();
                    match channel {
                        0 => match key {
                            "left" => hsv.saturation = (hsv.saturation - step / 100.).max(0.),
                            "right" => hsv.saturation = (hsv.saturation + step / 100.).min(1.),
                            "up" => hsv.value = (hsv.value + step / 100.).min(1.),
                            "down" => hsv.value = (hsv.value - step / 100.).max(0.),
                            "home" => {
                                hsv.saturation = 0.;
                                hsv.value = 1.;
                            }
                            "end" => hsv.value = 0.,
                            _ => return,
                        },
                        1 => match key {
                            "left" | "down" => hsv.hue = (hsv.hue - step / 360.).rem_euclid(1.),
                            "right" | "up" => hsv.hue = (hsv.hue + step / 360.).rem_euclid(1.),
                            "home" => hsv.hue = 0.,
                            "end" => hsv.hue = 359. / 360.,
                            _ => return,
                        },
                        _ => match key {
                            "left" | "down" => alpha = (alpha - step / 100.).max(0.),
                            "right" | "up" => alpha = (alpha + step / 100.).min(1.),
                            "home" => alpha = 0.,
                            "end" => alpha = 1.,
                            _ => return,
                        },
                    }
                    this.set_picker_color(&key_field, hsv, (alpha * 255.).round() as u8, cx);
                    cx.stop_propagation();
                }));
            picker = picker.child(control);
            if channel == 2 {
                picker = picker.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(format!("Opacity · {:.0}%", color.alpha() * 100.)),
                );
            }
        }
        let mut swatches = div().flex().flex_wrap().gap_2();
        let mut palette = vec![
            0x171717, 0xffffff, 0xfff7e6, 0xc6605c, 0xe6ad48, 0x55917e, 0x3265a8, 0x8d6eb5,
        ];
        for recent in &self.controller.settings.recent_colors {
            if !palette.contains(&recent.rgb()) {
                palette.push(recent.rgb());
            }
        }
        for value in palette {
            let draft = field.clone();
            swatches = swatches.child(
                self.control(
                    format!("picker-swatch-{value}"),
                    format!("Choose #{value:06X}"),
                    div()
                        .size(px(24.))
                        .rounded(px(3.))
                        .bg(rgb(value))
                        .border_1()
                        .border_color(theme.border)
                        .into_any_element(),
                    color.rgb() == value,
                    cx,
                    move |_, _, cx| {
                        let alpha = if opacity {
                            ThemeColor::parse(&draft.read(cx).content).map_or(255, |c| c.0 & 255)
                        } else {
                            255
                        };
                        draft.update(cx, |f, cx| {
                            f.set_content(ThemeColor((value << 8) | alpha).hex(), cx)
                        });
                    },
                )
                .size(px(34.))
                .p_0(),
            );
        }
        picker.child(swatches).child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(28.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(theme.border)
                        .bg(rgb(theme.surface))
                        .child(div().size_full().bg(rgba(color.0))),
                )
                .child(div().text_sm().child(color.hex().to_uppercase())),
        )
    }

    fn pick_color(
        &mut self,
        field: &Entity<Field>,
        channel: usize,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        opacity: bool,
        cx: &mut Context<Self>,
    ) {
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return;
        }
        let x =
            (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width)).clamp(0., 1.);
        let y =
            (f32::from(position.y - bounds.origin.y) / f32::from(bounds.size.height)).clamp(0., 1.);
        let color = ThemeColor::parse(&field.read(cx).content).unwrap_or(ThemeColor::opaque(
            Theme::new(&self.controller.settings).canvas.paper,
        ));
        let mut hsv = self.picker_hsv(field, color);
        let mut alpha = if opacity { (color.0 & 255) as u8 } else { 255 };
        match channel {
            0 => {
                hsv.saturation = x;
                hsv.value = 1. - y;
            }
            1 => hsv.hue = x.min(1. - f32::EPSILON),
            _ => alpha = (x * 255.).round() as u8,
        }
        self.set_picker_color(field, hsv, alpha, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Hsv, TextureKey, picker_pixels};
    use folio_app::appearance::ThemeColor;

    #[test]
    fn hsv_round_trips_rgb_and_alpha() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let color = ThemeColor((r << 24) | (g << 16) | (b << 8) | 73);
                    assert_eq!(Hsv::from_color(color, 0.).color(73), color);
                }
            }
        }
    }

    #[test]
    fn grayscale_keeps_hue_and_box_corners_are_white_hue_and_black() {
        let hue = 2. / 3.;
        for rgb in [0, 0x777777, 0xffffff] {
            assert_eq!(Hsv::from_color(ThemeColor::opaque(rgb), hue).hue, hue);
        }
        let base = Hsv {
            hue,
            saturation: 0.,
            value: 1.,
        };
        assert_eq!(base.color(255).rgb(), 0xffffff);
        assert_eq!(
            Hsv {
                saturation: 1.,
                ..base
            }
            .color(255)
            .rgb(),
            0x0000ff
        );
        assert_eq!(
            Hsv {
                saturation: 1.,
                value: 0.,
                ..base
            }
            .color(255)
            .rgb(),
            0
        );
    }

    #[test]
    fn saturation_value_bitmap_has_exact_opaque_bgra_at_selected_positions() {
        let key = TextureKey {
            hue: Some(2. / 3.),
            width: 100,
            height: 100,
        };
        let pixels = picker_pixels(key);
        // This pale blue was visibly too saturated with GPU gradients: the
        // red/green channels became about 122 instead of their selected 159.
        let index = (22 * key.width as usize + 19) * 4;
        assert_eq!(&pixels[index..index + 4], &[198, 159, 159, 255]);
        for row in 0..key.height {
            for col in 0..key.width {
                let selected = Hsv {
                    hue: key.hue.unwrap(),
                    saturation: (col as f32 + 0.5) / key.width as f32,
                    value: 1. - (row as f32 + 0.5) / key.height as f32,
                }
                .color(255)
                .0;
                let index = ((row * key.width + col) * 4) as usize;
                assert_eq!(
                    &pixels[index..index + 4],
                    &[
                        (selected >> 8) as u8,
                        (selected >> 16) as u8,
                        (selected >> 24) as u8,
                        255,
                    ]
                );
            }
        }
    }

    #[test]
    fn hue_bitmap_is_constant_vertically_and_keeps_srgb_secondary_colors() {
        let pixels = picker_pixels(TextureKey {
            hue: None,
            width: 12,
            height: 2,
        });
        assert_eq!(&pixels[..48], &pixels[48..]);
        // 15 degrees is orange, not the gamma-darkened red of the gradient.
        assert_eq!(&pixels[..4], &[0, 64, 255, 255]);
    }
}
