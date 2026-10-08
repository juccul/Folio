#[cfg(target_os = "linux")]
#[path = "../../apps/desktop/build_linux.rs"]
mod build_linux;
fn main() {
    #[cfg(target_os = "linux")]
    build_linux::link_libraries();
}
