//! Shared color controls edit a draft Field; Apply/Cancel keep existing semantics.
use super::*;
use folio_app::appearance::ThemeColor;
use std::{cell::Cell, rc::Rc};

fn replace_channel(color: ThemeColor, channel: usize, value: u8) -> ThemeColor {
    let shift = (3 - channel) * 8;
    ThemeColor((color.0 & !(255 << shift)) | ((value as u32) << shift))
}

impl NotesView {
    pub(super) fn color_selector(
        &mut self,
        field: Entity<Field>,
        opacity: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let color = ThemeColor::parse(&field.read(cx).content)
            .unwrap_or(ThemeColor::opaque(theme.canvas.paper));
        let mut swatches = div().flex().flex_wrap().gap_2();
        let mut palette = vec![
            0x171717, 0xffffff, 0xfff7e6, 0xc6605c, 0xe6ad48, 0x55917e, 0x3265a8, 0x8d6eb5,
        ];
        for recent in &self.controller.settings.recent_colors {
            if !palette.contains(&recent.rgb()) {
                palette.push(recent.rgb());
            }
        }
        for rgb_value in palette {
            let draft = field.clone();
            swatches = swatches.child(
                self.control(
                    format!("picker-swatch-{rgb_value}"),
                    format!("Choose #{rgb_value:06X}"),
                    div()
                        .size(px(24.))
                        .rounded_full()
                        .bg(rgb(rgb_value))
                        .border_1()
                        .border_color(theme.border)
                        .into_any_element(),
                    color.rgb() == rgb_value,
                    cx,
                    move |_, _, cx| {
                        let alpha = if opacity {
                            ThemeColor::parse(&draft.read(cx).content).map_or(255, |c| c.0 & 255)
                        } else {
                            255
                        };
                        draft.update(cx, |f, cx| {
                            f.set_content(ThemeColor((rgb_value << 8) | alpha).hex(), cx)
                        });
                    },
                )
                .size(px(34.))
                .p_0(),
            );
        }
        let mut picker = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(42.))
                            .rounded(px(theme.radius))
                            .border_1()
                            .border_color(theme.border)
                            .bg(rgb(theme.surface))
                            .child(
                                div()
                                    .size_full()
                                    .rounded(px(theme.radius))
                                    .bg(rgba(color.0)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(color.hex().to_uppercase())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .child("Drag to mix a color"),
                            ),
                    ),
            )
            .child(swatches);
        for (channel, label) in ["Red", "Green", "Blue", "Opacity"].into_iter().enumerate() {
            if channel == 3 && !opacity {
                break;
            }
            let shift = (3 - channel) * 8;
            let value = ((color.0 >> shift) & 255) as u8;
            let bounds = Rc::new(Cell::new(Bounds::default()));
            let measured = bounds.clone();
            let gradient = linear_gradient(
                90.,
                linear_color_stop(rgba(replace_channel(color, channel, 0).0), 0.),
                linear_color_stop(rgba(replace_channel(color, channel, 255).0), 1.),
            );
            let track = canvas(
                move |b, _, _| measured.set(b),
                move |b, _, window, _| {
                    window.paint_quad(fill(b, rgb(theme.surface)).corner_radii(px(4.)));
                    window.paint_quad(fill(b, gradient).corner_radii(px(4.)));
                    let x = b.origin.x + (b.size.width - px(4.)) * (value as f32 / 255.);
                    let marker = Bounds::new(
                        point(x, b.origin.y + px(2.)),
                        size(px(4.), b.size.height - px(4.)),
                    );
                    window.paint_quad(fill(marker, rgb(0xffffff)).corner_radii(px(2.)));
                    window.paint_quad(quad(
                        marker,
                        px(2.),
                        rgba(0),
                        px(1.),
                        rgb(0x171717),
                        BorderStyle::Solid,
                    ));
                },
            )
            .w_full()
            .h(px(26.));
            let down_field = field.clone();
            let move_field = field.clone();
            let key_field = field.clone();
            let down_bounds = bounds.clone();
            let slider = self
                .control(
                    format!("picker-channel-{channel}"),
                    format!("{label}: {value}. Left and right arrows adjust the value"),
                    track.into_any_element(),
                    false,
                    cx,
                    |_, _, _| {},
                )
                .flex_1()
                .min_w_0()
                .p_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.color_drag = Some((down_field.entity_id(), channel));
                        this.pick_channel(
                            &down_field,
                            channel,
                            event.position,
                            down_bounds.get(),
                            opacity,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                    if event.dragging()
                        && this.color_drag == Some((move_field.entity_id(), channel))
                    {
                        this.pick_channel(
                            &move_field,
                            channel,
                            event.position,
                            bounds.get(),
                            opacity,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.color_drag = None),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.color_drag = None),
                )
                .on_key_down(cx.listener(move |_, event: &KeyDownEvent, _, cx| {
                    let current = ThemeColor::parse(&key_field.read(cx).content).unwrap_or(color);
                    let value = ((current.0 >> shift) & 255) as u8;
                    let step = if event.keystroke.modifiers.shift {
                        10
                    } else {
                        1
                    };
                    let value = match event.keystroke.key.as_str() {
                        "left" | "down" => value.saturating_sub(step),
                        "right" | "up" => value.saturating_add(step),
                        "home" => 0,
                        "end" => 255,
                        _ => return,
                    };
                    key_field.update(cx, |f, cx| {
                        f.set_content(replace_channel(current, channel, value).hex(), cx)
                    });
                    cx.stop_propagation();
                }));
            picker = picker.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(54.)).text_xs().child(label))
                    .child(slider)
                    .child(div().w(px(28.)).text_xs().child(value.to_string())),
            );
        }
        picker.child(
            div()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child("Tab to a channel; use arrow keys to adjust. Shift adjusts by 10."),
        )
    }

    fn pick_channel(
        &mut self,
        field: &Entity<Field>,
        channel: usize,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        opacity: bool,
        cx: &mut Context<Self>,
    ) {
        if bounds.size.width <= px(0.) {
            return;
        }
        let fraction =
            (f32::from(position.x - bounds.origin.x) / f32::from(bounds.size.width)).clamp(0., 1.);
        let mut color = ThemeColor::parse(&field.read(cx).content).unwrap_or(ThemeColor::opaque(
            Theme::new(&self.controller.settings).canvas.paper,
        ));
        if !opacity {
            color.0 |= 255;
        }
        let color = replace_channel(color, channel, (fraction * 255.).round() as u8);
        field.update(cx, |field, cx| field.set_content(color.hex(), cx));
    }
}
