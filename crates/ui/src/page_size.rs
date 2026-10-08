use super::*;
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Unit {
    #[default]
    Millimeters,
    Inches,
    Pixels,
}
impl Unit {
    fn pixels(self) -> f32 {
        match self {
            Self::Millimeters => 96. / 25.4,
            Self::Inches => 96.,
            Self::Pixels => 1.,
        }
    }
    pub fn format(self, width: f32, height: f32) -> String {
        format!(
            "{:.4} × {:.4}",
            width / self.pixels(),
            height / self.pixels()
        )
    }
    pub fn parse(self, content: &str) -> Result<(f32, f32), &'static str> {
        let values = content
            .split(['×', 'x', 'X', ',', ' ', '\n', '\t'])
            .filter(|s| !s.is_empty())
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Enter width × height using numbers.")?;
        if values.len() != 2 {
            return Err("Enter exactly two dimensions: width × height.");
        }
        validation::page_size(&format!(
            "{} {}",
            values[0] * self.pixels(),
            values[1] * self.pixels()
        ))
    }
}
impl NotesView {
    pub(super) fn page_size_options(&self, cx: &mut Context<Self>) -> Div {
        let mut panel=div().flex().flex_col().gap_2().child(div().text_sm().child("Width × height. PDF preserves these physical dimensions. Canvas pixels use 96 pixels per inch."));
        let mut units = div().flex().flex_wrap().gap_2();
        for (id, label, unit) in [
            ("page-unit-mm", "Millimeters", Unit::Millimeters),
            ("page-unit-in", "Inches", Unit::Inches),
            ("page-unit-px", "Canvas pixels", Unit::Pixels),
        ] {
            units = units.child(
                self.button(
                    id,
                    label,
                    self.page_size_unit == unit,
                    cx,
                    move |this, w, cx| {
                        let Some((_, field)) = &this.modal else {
                            return;
                        };
                        match this.page_size_unit.parse(&field.read(cx).content) {
                            Ok((width, height)) => {
                                field.update(cx, |field, cx| {
                                    field.set_content(unit.format(width, height), cx)
                                });
                                this.page_size_unit = unit;
                                this.modal_error = None;
                            }
                            Err(e) => {
                                this.modal_error = Some(e.into());
                                field.read(cx).focus.focus(w);
                            }
                        }
                    },
                )
                .text_xs(),
            );
        }
        let mut sizes = div().flex().flex_wrap().gap_2();
        for (id, label, width, height) in [
            ("custom-size-a4", "A4", 210. / 25.4 * 96., 297. / 25.4 * 96.),
            ("custom-size-a5", "A5", 148. / 25.4 * 96., 210. / 25.4 * 96.),
            ("custom-size-letter", "Letter", 816., 1056.),
        ] {
            sizes = sizes.child(
                self.button(id, label, false, cx, move |this, _, cx| {
                    let landscape = this.controller.page().properties.width
                        > this.controller.page().properties.height;
                    let (width, height) = if landscape {
                        (height, width)
                    } else {
                        (width, height)
                    };
                    if let Some((_, field)) = &this.modal {
                        field.update(cx, |field, cx| {
                            field.set_content(this.page_size_unit.format(width, height), cx)
                        });
                        this.modal_error = None;
                    }
                })
                .text_xs(),
            );
        }
        panel = panel.child(units).child(sizes);
        panel
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn physical_page_units_match_pdf_points_and_reject_invalid_values() {
        let (w, h) = Unit::Millimeters.parse("210 × 297").unwrap();
        assert!((w * 0.75 - 595.2756).abs() < 0.001);
        assert!((h * 0.75 - 841.8898).abs() < 0.001);
        assert_eq!(Unit::Inches.parse("8.5 x 11"), Ok((816., 1056.)));
        for unit in [Unit::Millimeters, Unit::Inches, Unit::Pixels] {
            let text = unit.format(w, h);
            let (x, y) = unit.parse(&text).unwrap();
            assert!((x - w).abs() < 0.01 && (y - h).abs() < 0.01);
            assert!(unit.parse("NaN 100").is_err());
            assert!(unit.parse("0 100").is_err());
            assert!(unit.parse("100 100 extra").is_err());
        }
    }
}
