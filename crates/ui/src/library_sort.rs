use super::*;
pub(super) fn sort_notes(
    notes: &mut [folio_document::NoteMetadata],
    by_name: bool,
    reverse: bool,
    recent: bool,
) {
    if by_name {
        notes.sort_by_cached_key(|n| (n.title.to_lowercase(), n.id));
    } else if !recent {
        notes.sort_by_key(|n| (std::cmp::Reverse(n.updated_at), n.id));
    }
    if reverse {
        notes.reverse();
    }
}
impl NotesView {
    pub(super) fn sort_control(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let time = if self.controller.filter == NoteFilter::Recent {
            "Last opened"
        } else {
            "Last edited"
        };
        let choices = [
            ("sort-name-az", "Name: A–Z".into(), true, false),
            ("sort-name-za", "Name: Z–A".into(), true, true),
            (
                "sort-time-newest",
                format!("{time}: newest first"),
                false,
                false,
            ),
            (
                "sort-time-oldest",
                format!("{time}: oldest first"),
                false,
                true,
            ),
        ];
        let current = choices
            .iter()
            .find(|(_, _, name, reverse)| {
                *name == self.sort_by_name && *reverse == self.sort_reverse
            })
            .unwrap()
            .1
            .clone();
        let mut anchor = div().id("sort-anchor").relative().child(
            self.control(
                "sort",
                format!("Sort: {current}"),
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(current)
                    .child(icon(Icon::Down, theme.muted).size(rems(0.875)))
                    .into_any_element(),
                self.sort_open,
                cx,
                |this, _, _| this.sort_open = !this.sort_open,
            )
            .text_sm(),
        );
        if self.sort_open {
            let mut menu = div()
                .absolute()
                .left_0()
                .top_full()
                .w(rems(17.))
                .occlude()
                // The menu lies outside its anchor, so it closes on presses
                // outside itself; the Sort button toggles it on its own.
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    let (x, y) = (
                        f32::from(event.position.x) as f64,
                        f32::from(event.position.y) as f64,
                    );
                    if this
                        .accessibility
                        .control_bounds_for_key("sort")
                        .is_some_and(|r| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1)
                    {
                        return;
                    }
                    this.sort_open = false;
                    cx.notify();
                }))
                .p_2()
                .rounded(px(theme.corner_md()))
                .glass(theme, theme.popover)
                .flex()
                .flex_col()
                .gap_1();
            for (id, label, name, reverse) in choices {
                menu = menu.child(
                    self.button(
                        id,
                        label,
                        self.sort_by_name == name && self.sort_reverse == reverse,
                        cx,
                        move |this, _, _| {
                            this.sort_by_name = name;
                            this.sort_reverse = reverse;
                            this.sort_open = false;
                        },
                    )
                    .justify_start()
                    .text_xs(),
                );
            }
            let menu =
                self.transitions
                    .borrow_mut()
                    .overlay("library-sort-menu", self.sort_appear, menu);
            anchor = anchor.child(deferred(menu));
        }
        anchor
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn direction_and_recent_open_order_are_explicit_and_deterministic() {
        let mut a = folio_document::Document::new("zebra").metadata;
        a.updated_at = 10;
        let mut b = folio_document::Document::new("Apple").metadata;
        b.updated_at = 20;
        let mut notes = vec![a.clone(), b.clone()];
        sort_notes(&mut notes, true, false, false);
        assert_eq!(notes[0].id, b.id);
        sort_notes(&mut notes, true, true, false);
        assert_eq!(notes[0].id, a.id);
        sort_notes(&mut notes, false, false, false);
        assert_eq!(notes[0].id, b.id);
        sort_notes(&mut notes, false, true, false);
        assert_eq!(notes[0].id, a.id);
        notes = vec![a.clone(), b.clone()];
        sort_notes(&mut notes, false, false, true);
        assert_eq!(notes[0].id, a.id);
        sort_notes(&mut notes, false, true, true);
        assert_eq!(notes[0].id, b.id);
    }
}
