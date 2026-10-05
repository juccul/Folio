//! Offline validation and vector rendering of manually entered LaTeX equations.
use folio_document::*;
mod vector;
pub use vector::{VectorCommand, VectorFormula, VectorPath};
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
}
pub fn validate_latex(latex: &str) -> Result<(), Error> {
    if latex.is_empty() || latex.len() > 8192 {
        return Err(Error::Invalid("Equation must contain 1–8192 bytes".into()));
    }
    let forbidden = [
        "\\write",
        "\\input",
        "\\include",
        "\\open",
        "\\read",
        "\\catcode",
        "\\csname",
        "\\def",
        "\\usepackage",
    ];
    if forbidden.iter().any(|s| latex.contains(s)) {
        return Err(Error::Invalid("Only equation LaTeX is accepted".into()));
    }
    Ok(())
}
pub fn equation(latex: String, sources: Vec<Id>, rect: Rect) -> Result<Equation, Error> {
    validate_latex(&latex)?;
    let rendered_svg = Some(render(&latex)?);
    Ok(Equation {
        id: Id::new_v4(),
        latex,
        rendered_svg,
        rect,
        source_strokes: sources,
        transform: Transform::default(),
        math_link: None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_external_tex_commands() {
        assert!(validate_latex("\\input{/etc/passwd}").is_err());
        assert!(validate_latex("x^2 + \\frac{1}{2}").is_ok());
    }
}

/// Parse and render math on a background worker. The bundled STIX font uses OFL.
pub fn render(latex: &str) -> Result<String, Error> {
    validate_latex(latex)?;
    static FONT: std::sync::OnceLock<Result<latex_rust::MathFont, String>> =
        std::sync::OnceLock::new();
    let font = FONT
        .get_or_init(|| latex_rust::MathFont::stix_two_math().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Invalid(e.clone()))?;
    let mut options = latex_rust::SvgOptions::new();
    options.display = true;
    latex_rust::latex_to_svg(latex, font, &options)
        .map_err(|e| Error::Invalid(format!("Equation rendering: {e}")))
}
#[cfg(test)]
mod rendering_tests {
    use super::*;
    #[test]
    fn equation_retains_sources_and_has_vector_glyphs() {
        let source = Id::new_v4();
        let e = equation(
            r"x^2 + \frac{1}{2}".into(),
            vec![source],
            Rect::new(20., 30., 180., 60.),
        )
        .unwrap();
        assert_eq!(e.source_strokes, vec![source]);
        let svg = e.rendered_svg.unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("<svg"));
    }
    #[test]
    fn deeply_nested_math_is_rejected_without_aborting() {
        let latex = format!("{}x{}", "{".repeat(600), "}".repeat(600));
        assert!(render(&latex).is_err());
    }
}
