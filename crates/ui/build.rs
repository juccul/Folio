//! Converts the embedded SVG icons into Rune icon bodies: the root element's
//! paint attributes move onto a group around its children.
use std::{fmt::Write, fs, path::Path};

#[cfg(target_os = "linux")]
#[path = "../../apps/desktop/build_linux.rs"]
mod build_linux;

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn main() {
    #[cfg(target_os = "linux")]
    build_linux::link_libraries();
    let dir = Path::new("assets/icons");
    println!("cargo:rerun-if-changed=assets/icons");
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "svg"))
        .collect();
    names.sort();
    let mut out = String::new();
    let mut variants = Vec::new();
    for path in names {
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let source = fs::read_to_string(&path).unwrap();
        let source = source.split("-->").last().unwrap();
        let open = source.find("<svg").unwrap();
        let close = source[open..].find('>').unwrap() + open;
        let tag = source[open..close].replace(['\n', '\r'], " ");
        let body = &source[close + 1..source.rfind("</svg>").unwrap()];
        let view = attribute(&tag, "viewBox")
            .and_then(|v| v.split_whitespace().nth(2))
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(24.);
        let mut group = String::from("<g");
        for key in ["fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin"] {
            if let Some(value) = attribute(&tag, key) {
                write!(group, " {key}=\"{value}\"").unwrap();
            }
        }
        group.push('>');
        let body = format!("{group}{}</g>", body.split_whitespace().collect::<Vec<_>>().join(" "));
        writeln!(
            out,
            "static {}: rune::Icon = rune::Icon {{ name: {:?}, view: {view:?}, body: {:?} }};",
            name.to_uppercase(),
            name,
            body
        )
        .unwrap();
        variants.push(name);
    }
    writeln!(out, "#[allow(dead_code)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum Icon {{").unwrap();
    for v in &variants {
        writeln!(out, "    {v},").unwrap();
    }
    writeln!(out, "}}\n#[allow(dead_code)]\nimpl Icon {{\n    pub const ALL: &'static [Icon] = &[").unwrap();
    for v in &variants {
        writeln!(out, "        Icon::{v},").unwrap();
    }
    writeln!(out, "    ];\n    pub fn glyph(self) -> &'static rune::Icon {{\n        match self {{").unwrap();
    for v in &variants {
        writeln!(out, "            Icon::{v} => &{},", v.to_uppercase()).unwrap();
    }
    writeln!(out, "        }}\n    }}\n}}").unwrap();
    fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("icons.rs"), out).unwrap();
}
