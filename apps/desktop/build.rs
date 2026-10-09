#[cfg(target_os = "linux")]
mod build_linux;
// Allow linking against installed runtime libraries in minimal Linux environments.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/windows/folio.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/windows/folio.manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        let manifest = manifest.display().to_string().replace('\\', "\\\\");
        let resource =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("folio.rc");
        let icon = icon.display().to_string().replace('\\', "\\\\");
        let version = env!("CARGO_PKG_VERSION");
        let numbers = concat!(
            env!("CARGO_PKG_VERSION_MAJOR"),
            ",",
            env!("CARGO_PKG_VERSION_MINOR"),
            ",",
            env!("CARGO_PKG_VERSION_PATCH")
        );
        std::fs::write(
            &resource,
            format!(
                r#"
1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "Folio - offline handwriting notes"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "Folio"
      VALUE "ProductVersion", "{version}"
      VALUE "LegalCopyright", "GPL-3.0-or-later"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
            ),
        )
        .unwrap();
        embed_resource::compile(&resource, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
    #[cfg(target_os = "linux")]
    build_linux::link_libraries();
}
