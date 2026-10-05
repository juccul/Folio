//! Native Wayland pointer-gestures; no mouse emulation or stylus conversion.
use super::*;
use crate::NavigationGesture;
delegate_noop!(WaylandClientStatePtr: ignore zwp_pointer_gestures_v1::ZwpPointerGesturesV1);
impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,()> for WaylandClientStatePtr {
    fn event(this:&mut Self,_:&zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,event:zwp_pointer_gesture_pinch_v1::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
        let client=this.get_client();
        let mut state=client.borrow_mut();
        let (phase,dx,dy,scale,rotation,cancelled)=match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin{surface,..}=> {
                state.gesture_window=state.windows.get(&surface.id()).cloned();state.gesture_scale=1.;
                (TouchPhase::Started,0.,0.,1.,0.,false)
            }
            zwp_pointer_gesture_pinch_v1::Event::Update{dx,dy,scale,rotation,..}=> {
                let ratio=scale/state.gesture_scale.max(0.01);state.gesture_scale=scale;
                (TouchPhase::Moved,dx,dy,ratio,rotation,false)
            }
            zwp_pointer_gesture_pinch_v1::Event::End{cancelled,..}=>(TouchPhase::Ended,0.,0.,1.,0.,cancelled!=0),
            _=>return,
        };
        let window=state.gesture_window.clone();
        let position=state.mouse_location.unwrap_or_default();
        if matches!(phase,TouchPhase::Ended) {state.gesture_window=None;}
        drop(state);
        if let Some(window)=window {window.handle_input(PlatformInput::NavigationGesture(NavigationGesture {position,translation:point(px(dx as f32),px(dy as f32)),scale:scale as f32,rotation:(rotation as f32).to_radians(),phase,cancelled}));}
    }
}
