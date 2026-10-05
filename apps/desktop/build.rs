// Allow linking against installed runtime libraries in minimal Linux environments.
fn main() {
    #[cfg(target_os = "linux")]
    {
        use std::{env, fs, path::Path};
        let out = env::var("OUT_DIR").unwrap();
        let dir = Path::new(&out).join("native");
        fs::create_dir_all(&dir).unwrap();
        for lib in ["xkbcommon", "xkbcommon-x11", "xcb", "stdc++"] {
            for parent in ["/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/usr/lib"] {
                let source = Path::new(parent).join(format!("lib{lib}.so.0"));
                let source = if source.exists() {
                    source
                } else {
                    if lib == "stdc++" {
                        Path::new(parent).join("libstdc++.so.6")
                    } else {
                        Path::new(parent).join(format!("lib{lib}.so.1"))
                    }
                };
                let dest = dir.join(format!("lib{lib}.so"));
                if source.exists() && !dest.exists() {
                    std::os::unix::fs::symlink(source, &dest).unwrap();
                    break;
                }
            }
        }
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
}
