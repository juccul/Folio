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
        let mut anchor = div()
            .id("sort-anchor")
            .relative()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.sort_open = false;
                cx.notify();
            }))
            .child(
                self.button(
                    "sort",
                    format!("{current} ▾"),
                    self.sort_open,
                    cx,
                    |this, _, _| this.sort_open = !this.sort_open,
                )
                .text_xs(),
            );
        if self.sort_open {
            let mut menu = div()
                .absolute()
                .left_0()
                .top_full()
                .w(rems(17.))
                .occlude()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(rgb(theme.popover))
                .shadow_md()
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
