use super::*;

#[derive(Clone, Copy)]
pub(super) struct CanvasMenu {
    at: Point<Pixels>,
    pub(super) position: DocPoint,
    pub(super) can_paste: bool,
}
impl NotesView {
    pub(super) fn open_canvas_menu(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.library_open || self.blocking_overlay() || self.controller.loading_note() {
            return;
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        self.finish_inline_text(window, cx);
        self.controller.finish();
        self.dismiss_popovers();
        let position = self
            .controller
            .session()
            .viewport
            .to_document(DocPoint::new(
                f32::from(at.x - bounds.origin.x),
                f32::from(at.y - bounds.origin.y),
            ));
        let can_paste = !self.controller.read_only() && cx.read_from_clipboard().is_some_and(|item| {
            item.text().is_some_and(|text| !text.is_empty()) || item.entries().iter().any(|entry| {
                matches!(entry, ClipboardEntry::Image(image) if matches!(image.format(), ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp))
            })
        });
        self.canvas_menu = Some(CanvasMenu {
            at,
            position,
            can_paste,
        });
        self.focus.focus(window);
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn canvas_menu_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let menu = self.canvas_menu.unwrap();
        let theme = Theme::new(&self.controller.settings);
        let size = window.viewport_size();
        let width = 224. * self.controller.settings.ui_scale;
        let height = 52. * self.controller.settings.ui_scale;
        div()
            .id("canvas-context-menu")
            .occlude()
            .absolute()
            .left(px(
                f32::from(menu.at.x).clamp(8., (f32::from(size.width) - width - 8.).max(8.))
            ))
            .top(px(
                f32::from(menu.at.y).clamp(8., (f32::from(size.height) - height - 8.).max(8.))
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .w(px(width))
            .p_1()
            .rounded(px(10.))
            .bg(rgb(theme.popover))
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .child(
                self.control(
                    "canvas-paste",
                    "Paste",
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(icon(Icon::Paste, theme.ink).size(rems(1.125)))
                        .child(div().flex_1().child("Paste"))
                        .child(div().text_xs().text_color(rgb(theme.muted)).child("Ctrl+V"))
                        .into_any_element(),
                    false,
                    cx,
                    move |this, window, cx| {
                        this.canvas_menu = None;
                        this.paste_at(Some(menu.position), cx);
                        this.focus.focus(window);
                    },
                )
                .w_full()
                .justify_start()
                .text_sm()
                .px_3()
                .py_2()
                .min_h(rems(2.5))
                .rounded(px(6.))
                .bg(transparent_black())
                .hover(move |s| s.bg(rgb(theme.selected))),
            )
    }
}
