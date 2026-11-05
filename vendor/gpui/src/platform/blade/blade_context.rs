use anyhow::Context as _;
use blade_graphics as gpu;
use std::sync::Arc;
use util::ResultExt;

#[cfg_attr(target_os = "macos", derive(Clone))]
pub struct BladeContext {
    pub(super) gpu: Arc<gpu::Context>,
}

impl BladeContext {
    pub fn new() -> anyhow::Result<Self> {
        let device_id_forced = match std::env::var("ZED_DEVICE_ID") {
            Ok(val) => parse_pci_id(&val)
                .context("Failed to parse device ID from `ZED_DEVICE_ID` environment variable")
                .log_err(),
            Err(std::env::VarError::NotPresent) => None,
            err => {
                err.context("Failed to read value of `ZED_DEVICE_ID` environment variable")
                    .log_err();
                None
            }
        };
        let init = |device_id| unsafe {
            gpu::Context::init(gpu::ContextDesc {
                presentation: true,
                validation: false,
                device_id,
                ..Default::default()
            })
        };
        let gpu = match device_id_forced.or_else(display_gpu) {
            Some(device_id) => init(device_id).or_else(|err| {
                if device_id_forced.is_some() {
                    return Err(err);
                }
                log::info!("Display GPU {device_id:04x} unusable ({err:?}); using the default");
                init(0)
            }),
            None => init(0),
        }
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        log::info!("Rendering with {}", gpu.device_information().device_name);
        Ok(Self { gpu: Arc::new(gpu) })
    }
}

/// On a machine with several GPUs, the PCI device ID of the one the firmware
/// set up for the display, unless the user chose a GPU for Mesa. Without
/// this, a hybrid laptop can render on its discrete GPU and copy every frame
/// to the integrated one that drives the screen, which makes resizing slow.
fn display_gpu() -> Option<u32> {
    let chosen = [
        "DRI_PRIME",
        "MESA_VK_DEVICE_SELECT",
        "__NV_PRIME_RENDER_OFFLOAD",
    ];
    if chosen.iter().any(|name| std::env::var_os(name).is_some()) {
        return None;
    }
    let mut gpus = 0;
    let mut display = None;
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Cards, not their connectors (`card1-eDP-1`).
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        let Ok(id) = std::fs::read_to_string(device.join("device")) else {
            continue;
        };
        gpus += 1;
        if std::fs::read_to_string(device.join("boot_vga")).is_ok_and(|v| v.trim() == "1") {
            display = parse_pci_id(&id).ok();
        }
    }
    display.filter(|_| gpus > 1)
}

fn parse_pci_id(id: &str) -> anyhow::Result<u32> {
    let mut id = id.trim();

    if id.starts_with("0x") || id.starts_with("0X") {
        id = &id[2..];
    }
    let is_hex_string = id.chars().all(|c| c.is_ascii_hexdigit());
    let is_4_chars = id.len() == 4;
    anyhow::ensure!(
        is_4_chars && is_hex_string,
        "Expected a 4 digit PCI ID in hexadecimal format"
    );

    u32::from_str_radix(id, 16).context("parsing PCI ID as hex")
}

#[cfg(test)]
mod tests {
    use super::parse_pci_id;

    #[test]
    fn test_parse_device_id() {
        assert!(parse_pci_id("0xABCD").is_ok());
        assert!(parse_pci_id("ABCD").is_ok());
        assert!(parse_pci_id("abcd").is_ok());
        assert!(parse_pci_id("1234").is_ok());
        assert!(parse_pci_id("123").is_err());
        assert_eq!(
            parse_pci_id(&format!("{:x}", 0x1234)).unwrap(),
            parse_pci_id(&format!("{:X}", 0x1234)).unwrap(),
        );

        assert_eq!(
            parse_pci_id(&format!("{:#x}", 0x1234)).unwrap(),
            parse_pci_id(&format!("{:#X}", 0x1234)).unwrap(),
        );
        assert_eq!(
            parse_pci_id(&format!("{:#x}", 0x1234)).unwrap(),
            parse_pci_id(&format!("{:#X}", 0x1234)).unwrap(),
        );
    }
}
