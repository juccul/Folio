//! The GPU the compositor composites with, from `zwp_linux_dmabuf_v1`
//! default feedback. Rendering anywhere else can produce dmabufs the
//! compositor cannot import, and it then drops the connection.

use wayland_client::globals::GlobalList;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
};

#[derive(Default)]
struct MainDevice {
    device: Option<u64>,
    done: bool,
}

/// The compositor's main device as a `dev_t`, or `None` before
/// `zwp_linux_dmabuf_v1` version 4. Uses its own queue so no events of the
/// main queue are dispatched early.
pub(super) fn compositor_render_device(conn: &Connection, globals: &GlobalList) -> Option<u64> {
    let mut queue = conn.new_event_queue::<MainDevice>();
    let qh = queue.handle();
    let dmabuf: ZwpLinuxDmabufV1 = globals.bind(&qh, 4..=4, ()).ok()?;
    let feedback = dmabuf.get_default_feedback(&qh, ());
    let mut state = MainDevice::default();
    // The feedback normally arrives within one round trip; allow a second
    // for a compositor that sends `done` late, then stop either way.
    for _ in 0..2 {
        if state.done || queue.roundtrip(&mut state).is_err() {
            break;
        }
    }
    feedback.destroy();
    dmabuf.destroy();
    state.device
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for MainDevice {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxDmabufV1,
        _: <ZwpLinuxDmabufV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for MainDevice {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } => {
                state.device = parse_dev(&device);
            }
            zwp_linux_dmabuf_feedback_v1::Event::Done => state.done = true,
            _ => {}
        }
    }
}

/// `main_device` carries a native-endian `dev_t`.
fn parse_dev(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        8 => Some(u64::from_ne_bytes(bytes.try_into().ok()?)),
        4 => Some(u32::from_ne_bytes(bytes.try_into().ok()?).into()),
        _ => None,
    }
}
