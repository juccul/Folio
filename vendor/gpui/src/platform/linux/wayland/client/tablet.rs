//! Folio extension: native, frame-atomic Wayland tablet input. Apache-2.0.
use super::*;
use crate::{TabletEvent, TabletPhase};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_pad_group_v2, zwp_tablet_pad_ring_v2, zwp_tablet_pad_strip_v2, zwp_tablet_pad_v2,
    zwp_tablet_tool_v2, zwp_tablet_v2,
};
#[derive(Default)]
pub(super) struct TabletState {
    window: Option<WaylandWindowStatePtr>,
    position: Point<Pixels>,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    buttons: u32,
    eraser: bool,
    down: bool,
    pending: Vec<TabletPhase>,
    leaving: bool,
}
delegate_noop!(WaylandClientStatePtr: ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);
impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for WaylandClientStatePtr {
    fn event(
        _: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        _: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(WaylandClientStatePtr,zwp_tablet_seat_v2::ZwpTabletSeatV2,[
     zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE=>(zwp_tablet_v2::ZwpTabletV2,()),
     zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE=>(zwp_tablet_tool_v2::ZwpTabletToolV2,()),
     zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE=>(zwp_tablet_pad_v2::ZwpTabletPadV2,())
    ]);
}
impl Dispatch<zwp_tablet_v2::ZwpTabletV2, ()> for WaylandClientStatePtr {
    fn event(
        _: &mut Self,
        tablet: &zwp_tablet_v2::ZwpTabletV2,
        event: zwp_tablet_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_v2::Event::Removed) {
            tablet.destroy();
        }
    }
}
#[derive(Default)]
pub(super) struct PadState {window:Option<WaylandWindowStatePtr>}
pub(super) struct PadControl {pad:ObjectId,last:Option<f64>,pending:Option<f64>}
impl Dispatch<zwp_tablet_pad_v2::ZwpTabletPadV2, ()> for WaylandClientStatePtr {
    fn event(this:&mut Self,pad:&zwp_tablet_pad_v2::ZwpTabletPadV2,event:zwp_tablet_pad_v2::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
        let client=this.get_client();let mut state=client.borrow_mut();
        use zwp_tablet_pad_v2::Event;
        match event {
            Event::Enter {surface,..}=>{let window=get_window(&mut state,&surface.id());state.tablet_pads.entry(pad.id()).or_default().window=window;},
            Event::Leave {..}=>{if let Some(p)=state.tablet_pads.get_mut(&pad.id()) {p.window=None;}for c in state.tablet_pad_controls.values_mut().filter(|c|c.pad==pad.id()) {c.last=None;c.pending=None;}},
            Event::Group {pad_group}=>{state.tablet_pad_groups.insert(pad_group.id(),pad.id());},
            Event::Button {time,button,state:button_state}=> {
                let window=state.tablet_pads.get(&pad.id()).and_then(|p|p.window.clone());drop(state);
                if let Some(window)=window {window.handle_input(PlatformInput::TabletPad(crate::TabletPadEvent {button:Some(button),pressed:matches!(button_state,WEnum::Value(zwp_tablet_pad_v2::ButtonState::Pressed)),delta:0.,strip:false,timestamp:time}));}
            },
            Event::Removed=>{state.tablet_pads.remove(&pad.id());state.tablet_pad_controls.retain(|_,c|c.pad!=pad.id());state.tablet_pad_groups.retain(|_,p|*p!=pad.id());pad.destroy();},_=>{}
        }
    }
    event_created_child!(WaylandClientStatePtr,zwp_tablet_pad_v2::ZwpTabletPadV2,[zwp_tablet_pad_v2::EVT_GROUP_OPCODE=>(zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,())]);
}
impl Dispatch<zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()> for WaylandClientStatePtr {
    fn event(this:&mut Self,group:&zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,event:zwp_tablet_pad_group_v2::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
        let client=this.get_client();let mut state=client.borrow_mut();let Some(pad)=state.tablet_pad_groups.get(&group.id()).cloned() else {return;};
        let id=match event {zwp_tablet_pad_group_v2::Event::Ring {ring}=>ring.id(),zwp_tablet_pad_group_v2::Event::Strip {strip}=>strip.id(),_=>return};
        state.tablet_pad_controls.insert(id,PadControl {pad,last:None,pending:None});
    }
    event_created_child!(WaylandClientStatePtr,zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,[zwp_tablet_pad_group_v2::EVT_RING_OPCODE=>(zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,()),zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE=>(zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,())]);
}
fn pad_frame(this:&mut WaylandClientStatePtr,id:ObjectId,time:u32,strip:bool) {
    let client=this.get_client();let mut state=client.borrow_mut();
    let Some(control)=state.tablet_pad_controls.get_mut(&id) else {return;};
    let Some(value)=control.pending.take() else {return;};
    let previous=control.last.replace(value);let pad=control.pad.clone();
    let Some(previous)=previous else {return;};
    let delta=if strip {value-previous} else {(value-previous+180.).rem_euclid(360.)-180.};
    let window=state.tablet_pads.get(&pad).and_then(|p|p.window.clone());drop(state);
    if let Some(window)=window {window.handle_input(PlatformInput::TabletPad(crate::TabletPadEvent {button:None,pressed:false,delta:delta as f32,strip,timestamp:time}));}
}
impl Dispatch<zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,()> for WaylandClientStatePtr {
    fn event(this:&mut Self,ring:&zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,event:zwp_tablet_pad_ring_v2::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
        use zwp_tablet_pad_ring_v2::Event;
        if let Event::Frame {time}=event {pad_frame(this,ring.id(),time,false);return;}
        let client=this.get_client();let mut state=client.borrow_mut();if let Some(c)=state.tablet_pad_controls.get_mut(&ring.id()) {match event {Event::Angle {degrees}=>c.pending=Some(degrees),Event::Stop=>{c.last=None;c.pending=None;},_=>{}}}
    }
}
impl Dispatch<zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,()> for WaylandClientStatePtr {
    fn event(this:&mut Self,strip:&zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,event:zwp_tablet_pad_strip_v2::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
        use zwp_tablet_pad_strip_v2::Event;
        if let Event::Frame {time}=event {pad_frame(this,strip.id(),time,true);return;}
        let client=this.get_client();let mut state=client.borrow_mut();if let Some(c)=state.tablet_pad_controls.get_mut(&strip.id()) {match event {Event::Position {position}=>c.pending=Some(position as f64/65535.),Event::Stop=>{c.last=None;c.pending=None;},_=>{}}}
    }
}
impl Dispatch<zwp_tablet_tool_v2::ZwpTabletToolV2, ()> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        tool: &zwp_tablet_tool_v2::ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();
        let window = if let zwp_tablet_tool_v2::Event::ProximityIn { ref surface, .. } = event {
            get_window(&mut state, &surface.id())
        } else {
            None
        };
        let modifiers = state.modifiers;
        let t = state.tablet_tools.entry(tool.id()).or_default();
        use zwp_tablet_tool_v2::Event;
        match event {
            Event::Type { tool_type } => {
                t.eraser = matches!(tool_type, WEnum::Value(zwp_tablet_tool_v2::Type::Eraser))
            }
            Event::ProximityIn { .. } => {
                t.window = window;
                t.leaving = false;
                t.pressure = 0.;
                t.buttons = 0;
                t.down = false;
            }
            Event::Motion { x, y } => t.position = point(px(x as f32), px(y as f32)),
            Event::Pressure { pressure } => t.pressure = pressure as f32 / 65535.,
            Event::Tilt { tilt_x, tilt_y } => {
                t.tilt_x = tilt_x as f32;
                t.tilt_y = tilt_y as f32;
            }
            Event::Down { .. } => {
                t.down = true;
                t.pending.push(TabletPhase::Down);
            }
            Event::Up => {
                t.down = false;
                t.pending.push(TabletPhase::Up);
            }
            Event::Button {
                button,
                state: button_state,
                ..
            } => {
                let bit = match button {
                    0x14b => 1,
                    0x14c => 2,
                    _ => 4,
                };
                if matches!(
                    button_state,
                    WEnum::Value(zwp_tablet_tool_v2::ButtonState::Pressed)
                ) {
                    t.buttons |= bit
                } else {
                    t.buttons &= !bit
                }
            }
            Event::ProximityOut => {
                t.leaving = true;
                t.pending.push(TabletPhase::Leave);
            }
            Event::Frame { time } => {
                let Some(window) = t.window.clone() else {
                    t.pending.clear();
                    return;
                };
                let phases = if t.pending.is_empty() {
                    vec![if t.down {
                        TabletPhase::Move
                    } else {
                        TabletPhase::Hover
                    }]
                } else {
                    std::mem::take(&mut t.pending)
                };
                let events = phases
                    .into_iter()
                    .map(|phase| TabletEvent {
                        position: t.position,
                        pressure: t.pressure,
                        tilt_x: t.tilt_x,
                        tilt_y: t.tilt_y,
                        timestamp: time,
                        buttons: t.buttons,
                        eraser: t.eraser,
                        phase,
                        modifiers,
                    })
                    .collect::<Vec<_>>();
                if t.leaving {
                    t.window = None;
                    t.down = false;
                    t.buttons = 0;
                }
                drop(state);
                for event in events {
                    // Rich frames drive ink. Pointer-compatible events additionally let tablet
                    // tips activate ordinary GPUI controls; the canvas suppresses this duplicate.
                    window.handle_input(PlatformInput::Tablet(event.clone()));
                    let input = match event.phase {
                        TabletPhase::Down => Some(PlatformInput::MouseDown(MouseDownEvent {
                            button: MouseButton::Left,
                            position: event.position,
                            modifiers: event.modifiers,
                            click_count: 1,
                            first_mouse: false,
                        })),
                        TabletPhase::Up => Some(PlatformInput::MouseUp(MouseUpEvent {
                            button: MouseButton::Left,
                            position: event.position,
                            modifiers: event.modifiers,
                            click_count: 1,
                        })),
                        TabletPhase::Hover | TabletPhase::Move => {
                            Some(PlatformInput::MouseMove(MouseMoveEvent {
                                position: event.position,
                                pressed_button: if event.phase == TabletPhase::Move {
                                    Some(MouseButton::Left)
                                } else {
                                    None
                                },
                                modifiers: event.modifiers,
                            }))
                        }
                        _ => None,
                    };
                    if let Some(input) = input {
                        window.handle_input(input);
                    }
                }
            }
            Event::Removed => {
                let window = t.window.clone();
                let event = TabletEvent {
                    position: t.position,
                    pressure: t.pressure,
                    tilt_x: t.tilt_x,
                    tilt_y: t.tilt_y,
                    timestamp: 0,
                    buttons: 0,
                    eraser: t.eraser,
                    phase: TabletPhase::Cancel,
                    modifiers,
                };
                state.tablet_tools.remove(&tool.id());
                drop(state);
                tool.destroy();
                if let Some(window) = window {
                    window.handle_input(PlatformInput::Tablet(event));
                }
            }
            _ => {}
        }
    }
}
