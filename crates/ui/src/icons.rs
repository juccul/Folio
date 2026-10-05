//! Original, small vector icons. No icon font, remote assets or runtime files.
use gpui::{prelude::*, *};

#[derive(Clone, Copy)]
pub enum Icon {
    Library,
    Book,
    Folder,
    Star,
    Clock,
    Trash,
    Search,
    Settings,
    Plus,
    Back,
    Forward,
    Down,
    Close,
    More,
    Sidebar,
    Pen,
    Pencil,
    Marker,
    Highlighter,
    Eraser,
    Lasso,
    Rectangle,
    Shapes,
    Text,
    Image,
    Hand,
    Undo,
    Redo,
    Export,
    Import,
    Check,
    Grid,
    List,
    Sliders,
    Help,
}

fn circle(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    (0..=32)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 32.;
            (x + r * a.cos(), y + r * a.sin())
        })
        .collect()
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
}

pub fn icon(kind: Icon, color: u32) -> impl IntoElement {
    use Icon::*;
    let paths: Vec<Vec<(f32, f32)>> = match kind {
        Library => vec![
            rect(3., 4., 5., 16.),
            rect(10., 4., 4., 16.),
            vec![(17., 4.), (21., 19.), (17., 20.), (14., 5.), (17., 4.)],
        ],
        Book => vec![
            rect(5., 3., 14., 18.),
            vec![(8., 3.), (8., 21.)],
            vec![(11., 8.), (16., 8.)],
        ],
        Folder => vec![vec![
            (3., 7.),
            (3., 19.),
            (21., 19.),
            (21., 7.),
            (12., 7.),
            (10., 4.),
            (3., 4.),
            (3., 7.),
        ]],
        Star => vec![
            (0..=10)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::PI / 5. - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 9. } else { 4.2 };
                    (12. + r * a.cos(), 12. + r * a.sin())
                })
                .collect(),
        ],
        Clock => vec![
            circle(12., 12., 9.),
            vec![(12., 6.), (12., 12.), (16., 14.)],
        ],
        Trash => vec![
            vec![(4., 6.), (20., 6.)],
            vec![(9., 6.), (9., 3.), (15., 3.), (15., 6.)],
            vec![(6., 6.), (7., 21.), (17., 21.), (18., 6.)],
            vec![(10., 10.), (10., 17.)],
            vec![(14., 10.), (14., 17.)],
        ],
        Search => vec![circle(10., 10., 6.5), vec![(15., 15.), (21., 21.)]],
        Settings => vec![
            circle(12., 12., 7.),
            circle(12., 12., 2.5),
            vec![(12., 2.), (12., 5.)],
            vec![(12., 19.), (12., 22.)],
            vec![(2., 12.), (5., 12.)],
            vec![(19., 12.), (22., 12.)],
            vec![(5., 5.), (7., 7.)],
            vec![(17., 17.), (19., 19.)],
            vec![(5., 19.), (7., 17.)],
            vec![(17., 7.), (19., 5.)],
        ],
        Plus => vec![vec![(12., 5.), (12., 19.)], vec![(5., 12.), (19., 12.)]],
        Back => vec![vec![(15., 5.), (8., 12.), (15., 19.)]],
        Forward => vec![vec![(9., 5.), (16., 12.), (9., 19.)]],
        Down => vec![vec![(6., 9.), (12., 15.), (18., 9.)]],
        Close => vec![vec![(6., 6.), (18., 18.)], vec![(18., 6.), (6., 18.)]],
        More => vec![
            circle(5., 12., 0.8),
            circle(12., 12., 0.8),
            circle(19., 12., 0.8),
        ],
        Sidebar => vec![
            rect(3., 4., 18., 16.),
            vec![(9., 4.), (9., 20.)],
            vec![(5., 8.), (7., 8.)],
            vec![(5., 12.), (7., 12.)],
        ],
        Pen => vec![
            vec![
                (4., 20.),
                (7., 13.),
                (17., 3.),
                (21., 7.),
                (11., 17.),
                (4., 20.),
            ],
            vec![(14., 6.), (18., 10.)],
            vec![(7., 13.), (11., 17.)],
        ],
        Pencil => vec![
            vec![
                (4., 20.),
                (6., 13.),
                (17., 2.),
                (22., 7.),
                (11., 18.),
                (4., 20.),
            ],
            vec![(14., 5.), (19., 10.)],
            vec![(6., 13.), (11., 18.)],
            vec![(9., 15.), (17., 7.)],
        ],
        Marker | Highlighter => vec![
            vec![(5., 16.), (12., 9.), (17., 14.), (10., 21.), (5., 16.)],
            vec![(12., 9.), (17., 3.), (22., 8.), (17., 14.)],
            vec![(5., 16.), (2., 19.), (4., 21.), (7., 18.)],
            vec![(2., 22.), (14., 22.)],
        ],
        Eraser => vec![
            vec![
                (3., 14.),
                (13., 4.),
                (21., 12.),
                (12., 21.),
                (10., 21.),
                (3., 14.),
            ],
            vec![(8., 9.), (16., 17.)],
            vec![(10., 21.), (21., 21.)],
        ],
        Lasso => vec![
            (0..=40)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 40.;
                    (12. + 9. * a.cos(), 10. + 6. * a.sin())
                })
                .collect(),
            vec![
                (8., 15.),
                (7., 18.),
                (10., 20.),
                (13., 18.),
                (11., 15.),
                (8., 15.),
            ],
            vec![(10., 20.), (15., 22.)],
        ],
        Rectangle => vec![
            vec![(8., 4.), (4., 4.), (4., 8.)],
            vec![(16., 4.), (20., 4.), (20., 8.)],
            vec![(20., 16.), (20., 20.), (16., 20.)],
            vec![(8., 20.), (4., 20.), (4., 16.)],
            vec![(11., 4.), (13., 4.)],
            vec![(20., 11.), (20., 13.)],
            vec![(11., 20.), (13., 20.)],
            vec![(4., 11.), (4., 13.)],
        ],
        Shapes => vec![
            vec![(3., 12.), (9., 2.), (15., 12.), (3., 12.)],
            rect(10., 13., 10., 8.),
        ],
        Text => vec![
            vec![(3., 5.), (21., 5.)],
            vec![(12., 5.), (12., 21.)],
            vec![(8., 21.), (16., 21.)],
            vec![(3., 5.), (3., 8.)],
            vec![(21., 5.), (21., 8.)],
        ],
        Image => vec![
            rect(3., 4., 18., 16.),
            circle(8., 9., 1.5),
            vec![(3., 17.), (9., 12.), (13., 16.), (17., 11.), (21., 15.)],
        ],
        Hand => vec![vec![
            (5., 13.),
            (5., 9.),
            (7., 8.),
            (9., 13.),
            (9., 4.),
            (11., 3.),
            (12., 4.),
            (12., 11.),
            (12., 3.),
            (14., 2.),
            (15., 3.),
            (15., 11.),
            (15., 5.),
            (17., 4.),
            (18., 5.),
            (18., 12.),
            (18., 9.),
            (20., 8.),
            (21., 9.),
            (21., 15.),
            (18., 21.),
            (10., 21.),
            (5., 13.),
        ]],
        Undo => vec![
            vec![(8., 5.), (3., 10.), (8., 15.)],
            vec![
                (3., 10.),
                (14., 10.),
                (19., 12.),
                (20., 16.),
                (18., 20.),
                (14., 21.),
            ],
        ],
        Redo => vec![
            vec![(16., 5.), (21., 10.), (16., 15.)],
            vec![
                (21., 10.),
                (10., 10.),
                (5., 12.),
                (4., 16.),
                (6., 20.),
                (10., 21.),
            ],
        ],
        Export => vec![
            vec![(12., 16.), (12., 3.)],
            vec![(7., 8.), (12., 3.), (17., 8.)],
            vec![(4., 13.), (4., 21.), (20., 21.), (20., 13.)],
        ],
        Import => vec![
            vec![(12., 3.), (12., 16.)],
            vec![(7., 11.), (12., 16.), (17., 11.)],
            vec![(4., 13.), (4., 21.), (20., 21.), (20., 13.)],
        ],
        Check => vec![vec![(5., 12.), (10., 17.), (20., 6.)]],
        Grid => vec![
            rect(3., 3., 7., 7.),
            rect(14., 3., 7., 7.),
            rect(3., 14., 7., 7.),
            rect(14., 14., 7., 7.),
        ],
        List => vec![
            vec![(8., 5.), (21., 5.)],
            vec![(8., 12.), (21., 12.)],
            vec![(8., 19.), (21., 19.)],
            circle(3., 5., 0.6),
            circle(3., 12., 0.6),
            circle(3., 19., 0.6),
        ],
        Sliders => vec![
            vec![(4., 6.), (20., 6.)],
            vec![(4., 12.), (20., 12.)],
            vec![(4., 18.), (20., 18.)],
            circle(8., 6., 2.),
            circle(16., 12., 2.),
            circle(10., 18., 2.),
        ],

        Help => vec![
            circle(12., 12., 9.),
            vec![
                (8., 8.),
                (10., 6.),
                (14., 6.),
                (16., 8.),
                (16., 10.),
                (12., 13.),
                (12., 15.),
            ],
            circle(12., 18., 0.5),
        ],
    };
    let paths = paths
        .into_iter()
        .filter_map(|points| {
            let mut b = PathBuilder::stroke(px(1.65));
            b.move_to(point(px(points[0].0), px(points[0].1)));
            for (x, y) in points.into_iter().skip(1) {
                b.line_to(point(px(x), px(y)));
            }
            b.build().ok()
        })
        .collect::<Vec<_>>();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let sx = f32::from(bounds.size.width) / 24.;
            let sy = f32::from(bounds.size.height) / 24.;
            if matches!(kind, Icon::More) {
                // Filled dots stay legible at fractional scale factors. Tiny
                // stroked circles can collapse during tessellation.
                for x in [5., 12., 19.] {
                    let dot = Bounds::new(
                        point(
                            bounds.origin.x + px((x - 1.2) * sx),
                            bounds.origin.y + px(10.8 * sy),
                        ),
                        size(px(2.4 * sx), px(2.4 * sy)),
                    );
                    window.paint_quad(fill(dot, rgb(color)).corner_radii(px(1.2 * sx)));
                }
                return;
            }
            for path in &paths {
                window.paint_path(
                    path.clone().transformed([
                        sx,
                        0.,
                        0.,
                        sy,
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                    ]),
                    rgb(color),
                );
            }
        },
    )
    .size(px(22.))
    .flex_shrink_0()
}
