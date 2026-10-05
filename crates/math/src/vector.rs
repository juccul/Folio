//! Font outlines in document coordinates, ready for a native vector painter.
use crate::Error;
use resvg::{tiny_skia, usvg};

#[derive(Clone, Debug)]
pub enum VectorCommand {
    Move([f32; 2]),
    Line([f32; 2]),
    Quad([f32; 2], [f32; 2]),
    Cubic([f32; 2], [f32; 2], [f32; 2]),
    Close,
}
#[derive(Clone, Debug)]
pub struct VectorPath {
    pub commands: Vec<VectorCommand>,
    pub even_odd: bool,
}
#[derive(Clone, Debug)]
pub struct VectorFormula {
    pub width: f32,
    pub height: f32,
    pub paths: Vec<VectorPath>,
}
impl VectorFormula {
    /// Parse only the outlined SVG produced by our bundled LaTeX renderer.
    /// This runs on the math worker; no SVG or font parsing happens in paint.
    pub fn from_svg(svg: &str) -> Result<Self, Error> {
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
            .map_err(|e| Error::Invalid(format!("Equation outlines: {e}")))?;
        let mut formula = Self {
            width: tree.size().width(),
            height: tree.size().height(),
            paths: Vec::new(),
        };
        let mut remaining = 250_000;
        collect(tree.root(), &mut formula.paths, &mut remaining)?;
        if formula.paths.is_empty() {
            return Err(Error::Invalid("Equation has no visible symbols".into()));
        }
        Ok(formula)
    }
}
fn collect(
    group: &usvg::Group,
    paths: &mut Vec<VectorPath>,
    remaining: &mut usize,
) -> Result<(), Error> {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect(group, paths, remaining)?,
            usvg::Node::Path(path) => {
                let Some(fill) = path.fill() else {
                    return Err(Error::Invalid("Equation requires filled outlines".into()));
                };
                let transform = path.abs_transform();
                let point = |mut p: tiny_skia::Point| {
                    transform.map_point(&mut p);
                    [p.x, p.y]
                };
                let mut commands = Vec::new();
                for segment in path.data().segments() {
                    if *remaining == 0 {
                        return Err(Error::Invalid("Equation has too many outlines".into()));
                    }
                    *remaining -= 1;
                    use tiny_skia::PathSegment::*;
                    commands.push(match segment {
                        MoveTo(p) => VectorCommand::Move(point(p)),
                        LineTo(p) => VectorCommand::Line(point(p)),
                        QuadTo(a, b) => VectorCommand::Quad(point(a), point(b)),
                        CubicTo(a, b, c) => VectorCommand::Cubic(point(a), point(b), point(c)),
                        Close => VectorCommand::Close,
                    });
                }
                paths.push(VectorPath {
                    commands,
                    even_odd: fill.rule() == usvg::FillRule::EvenOdd,
                });
            }
            _ => {
                return Err(Error::Invalid(
                    "Equation contains unsupported outlines".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_transforms_and_glyph_holes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="40"><g transform="translate(5 7)"><path fill-rule="evenodd" d="M0 0 L10 0 L10 10 Z M2 2 L3 3 L2 3 Z"/></g></svg>"#;
        let formula = VectorFormula::from_svg(svg).unwrap();
        assert_eq!((formula.width, formula.height), (30., 40.));
        assert!(formula.paths[0].even_odd);
        assert!(matches!(
            formula.paths[0].commands[0],
            VectorCommand::Move([5., 7.])
        ));
        assert_eq!(
            formula.paths[0]
                .commands
                .iter()
                .filter(|c| matches!(c, VectorCommand::Close))
                .count(),
            2
        );
    }
    #[test]
    fn fractions_roots_matrices_and_integrals_have_native_outlines() {
        for latex in [
            r"\frac{1}{2}",
            r"\sqrt{x^2+1}",
            r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
            r"\int_0^1 x^2\,dx",
            r"x=4",
        ] {
            let formula = VectorFormula::from_svg(&crate::render(latex).unwrap()).unwrap();
            assert!(formula.width > 0. && formula.height > 0.);
            assert!(!formula.paths.is_empty());
        }
        assert!(VectorFormula::from_svg("invalid").is_err());
    }
}
