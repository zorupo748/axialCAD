//! Pointer gestures (swipe and pinch).

use std::collections::VecDeque;
use std::sync::atomic::Ordering;

use sctk::compositor::SurfaceData;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gesture_swipe_v1::{self, ZwpPointerGestureSwipeV1},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
};

use crate::event::{TouchPhase, WindowEvent};
use crate::platform::wayland::{CancellationOrigin, GesturePhase, WaylandPointerGesture};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::WindowId;

/// Bounded queue of gestures with overflow protection.
#[derive(Debug, Clone)]
pub struct GestureBuffer {
    events: VecDeque<WaylandPointerGesture>,
    active_swipe: bool,
    active_pinch: bool,
    max_capacity: usize,
}

impl Default for GestureBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureBuffer {
    pub const DEFAULT_CAPACITY: usize = 128;

    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(Self::DEFAULT_CAPACITY),
            active_swipe: false,
            active_pinch: false,
            max_capacity: Self::DEFAULT_CAPACITY,
        }
    }

    pub fn push(&mut self, gesture: WaylandPointerGesture) {
        match &gesture {
            WaylandPointerGesture::Swipe { phase, .. } => match phase {
                GesturePhase::Started => self.active_swipe = true,
                GesturePhase::Ended | GesturePhase::Cancelled(_) => self.active_swipe = false,
                _ => {}
            },
            WaylandPointerGesture::Pinch { phase, .. } => match phase {
                GesturePhase::Started => self.active_pinch = true,
                GesturePhase::Ended | GesturePhase::Cancelled(_) => self.active_pinch = false,
                _ => {}
            },
        }

        if self.events.len() >= self.max_capacity {
            // Evict an intermediate update event to preserve start/end lifecycle boundaries
            if let Some(pos) = self.events.iter().position(|e| match e {
                WaylandPointerGesture::Swipe { phase: GesturePhase::Updated, .. } => true,
                WaylandPointerGesture::Pinch { phase: GesturePhase::Updated, .. } => true,
                _ => false,
            }) {
                self.events.remove(pos);
            } else {
                self.events.pop_front();
            }
        }

        self.events.push_back(gesture);
    }

    pub fn cancel_active_gestures(&mut self, origin: CancellationOrigin) {
        if self.active_swipe {
            self.active_swipe = false;
            self.push(WaylandPointerGesture::Swipe {
                phase: GesturePhase::Cancelled(origin),
                fingers: 0,
                delta: (0.0, 0.0),
            });
        }
        if self.active_pinch {
            self.active_pinch = false;
            self.push(WaylandPointerGesture::Pinch {
                phase: GesturePhase::Cancelled(origin),
                fingers: 0,
                delta: (0.0, 0.0),
                scale: 1.0,
                rotation_degrees: 0.0,
            });
        }
    }

    pub fn drain(&mut self) -> Vec<WaylandPointerGesture> {
        self.events.drain(..).collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveSwipe {
    pub window_id: WindowId,
    pub fingers: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActivePinch {
    pub window_id: WindowId,
    pub fingers: u32,
    pub last_scale: f64,
}

pub struct PointerGesturesState {
    manager: ZwpPointerGesturesV1,
    pub(crate) active_swipe: Option<ActiveSwipe>,
    pub(crate) active_pinch: Option<ActivePinch>,
}

impl PointerGesturesState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=3, GlobalData)?;
        Ok(Self { manager, active_swipe: None, active_pinch: None })
    }

    pub fn get_swipe_gesture(
        &self,
        pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ZwpPointerGestureSwipeV1 {
        self.manager.get_swipe_gesture(pointer, queue_handle, GlobalData)
    }

    pub fn get_pinch_gesture(
        &self,
        pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ZwpPointerGesturePinchV1 {
        self.manager.get_pinch_gesture(pointer, queue_handle, GlobalData)
    }
}

impl Dispatch<ZwpPointerGesturesV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        _state: &mut WinitState,
        _proxy: &ZwpPointerGesturesV1,
        _event: <ZwpPointerGesturesV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpPointerGestureSwipeV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        state: &mut WinitState,
        _proxy: &ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin { surface, fingers, .. } => {
                let parent_surface = match surface.data::<SurfaceData>() {
                    Some(data) => data.parent_surface().unwrap_or(&surface),
                    None => &surface,
                };
                let window_id = crate::platform_impl::wayland::make_wid(parent_surface);

                if let Some(gestures) = state.pointer_gestures.as_mut() {
                    gestures.active_swipe = Some(ActiveSwipe { window_id, fingers });
                }

                if let Some(window) = state.windows.get_mut().get(&window_id) {
                    let mut win = window.lock().unwrap();
                    win.gesture_buffer.push(WaylandPointerGesture::Swipe {
                        phase: GesturePhase::Started,
                        fingers,
                        delta: (0.0, 0.0),
                    });
                }

                if let Some(requests) = state.window_requests.borrow().get(&window_id) {
                    requests.redraw_requested.store(true, Ordering::Relaxed);
                }
                state.events_sink.push_window_event(WindowEvent::RedrawRequested, window_id);
            },
            zwp_pointer_gesture_swipe_v1::Event::Update { dx, dy, .. } => {
                let active = state.pointer_gestures.as_ref().and_then(|g| g.active_swipe);
                if let Some(active) = active {
                    if let Some(window) = state.windows.get_mut().get(&active.window_id) {
                        let mut win = window.lock().unwrap();
                        win.gesture_buffer.push(WaylandPointerGesture::Swipe {
                            phase: GesturePhase::Updated,
                            fingers: active.fingers,
                            delta: (dx, dy),
                        });
                        if let Some(requests) =
                            state.window_requests.borrow().get(&active.window_id)
                        {
                            requests.redraw_requested.store(true, Ordering::Relaxed);
                        }
                        state
                            .events_sink
                            .push_window_event(WindowEvent::RedrawRequested, active.window_id);
                    } else if let Some(gestures) = state.pointer_gestures.as_mut() {
                        gestures.active_swipe = None;
                    }
                }
            },
            zwp_pointer_gesture_swipe_v1::Event::End { cancelled, .. } => {
                let active = state.pointer_gestures.as_mut().and_then(|g| g.active_swipe.take());
                if let Some(active) = active {
                    if let Some(window) = state.windows.get_mut().get(&active.window_id) {
                        let mut win = window.lock().unwrap();
                        let phase = if cancelled != 0 {
                            GesturePhase::Cancelled(CancellationOrigin::CompositorProtocol)
                        } else {
                            GesturePhase::Ended
                        };
                        win.gesture_buffer.push(WaylandPointerGesture::Swipe {
                            phase,
                            fingers: active.fingers,
                            delta: (0.0, 0.0),
                        });
                        if let Some(requests) =
                            state.window_requests.borrow().get(&active.window_id)
                        {
                            requests.redraw_requested.store(true, Ordering::Relaxed);
                        }
                        state
                            .events_sink
                            .push_window_event(WindowEvent::RedrawRequested, active.window_id);
                    }
                }
            },
            _ => {},
        }
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        state: &mut WinitState,
        _proxy: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { surface, fingers, .. } => {
                let parent_surface = match surface.data::<SurfaceData>() {
                    Some(data) => data.parent_surface().unwrap_or(&surface),
                    None => &surface,
                };
                let window_id = crate::platform_impl::wayland::make_wid(parent_surface);

                if let Some(gestures) = state.pointer_gestures.as_mut() {
                    gestures.active_pinch =
                        Some(ActivePinch { window_id, fingers, last_scale: 1.0 });
                }

                if let Some(window) = state.windows.get_mut().get(&window_id) {
                    let mut win = window.lock().unwrap();
                    win.gesture_buffer.push(WaylandPointerGesture::Pinch {
                        phase: GesturePhase::Started,
                        fingers,
                        delta: (0.0, 0.0),
                        scale: 1.0,
                        rotation_degrees: 0.0,
                    });
                }

                if fingers == 2 {
                    let device_id = crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                        crate::platform_impl::wayland::DeviceId,
                    ));
                    state.events_sink.push_window_event(
                        WindowEvent::PinchGesture {
                            device_id,
                            delta: 0.0,
                            phase: TouchPhase::Started,
                        },
                        window_id,
                    );
                }

                if let Some(requests) = state.window_requests.borrow().get(&window_id) {
                    requests.redraw_requested.store(true, Ordering::Relaxed);
                }
                state.events_sink.push_window_event(WindowEvent::RedrawRequested, window_id);
            },
            zwp_pointer_gesture_pinch_v1::Event::Update { dx, dy, scale, rotation, .. } => {
                let active_info = state.pointer_gestures.as_ref().and_then(|g| g.active_pinch);
                if let Some(mut active) = active_info {
                    let ratio = if scale.is_finite() && active.last_scale > 0.0 && scale > 0.0 {
                        scale / active.last_scale
                    } else {
                        1.0
                    };
                    let pinch_delta = ratio.ln();
                    active.last_scale = scale;
                    if let Some(gestures) = state.pointer_gestures.as_mut() {
                        gestures.active_pinch = Some(active);
                    }

                    if let Some(window) = state.windows.get_mut().get(&active.window_id) {
                        let mut win = window.lock().unwrap();
                        win.gesture_buffer.push(WaylandPointerGesture::Pinch {
                            phase: GesturePhase::Updated,
                            fingers: active.fingers,
                            delta: (dx, dy),
                            scale,
                            rotation_degrees: rotation,
                        });

                        if active.fingers == 2 {
                            let device_id =
                                crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                                    crate::platform_impl::wayland::DeviceId,
                                ));
                            state.events_sink.push_window_event(
                                WindowEvent::PinchGesture {
                                    device_id,
                                    delta: pinch_delta,
                                    phase: TouchPhase::Moved,
                                },
                                active.window_id,
                            );
                        }

                        if let Some(requests) =
                            state.window_requests.borrow().get(&active.window_id)
                        {
                            requests.redraw_requested.store(true, Ordering::Relaxed);
                        }
                        state
                            .events_sink
                            .push_window_event(WindowEvent::RedrawRequested, active.window_id);
                    } else if let Some(gestures) = state.pointer_gestures.as_mut() {
                        gestures.active_pinch = None;
                    }
                }
            },
            zwp_pointer_gesture_pinch_v1::Event::End { cancelled, .. } => {
                let active = state.pointer_gestures.as_mut().and_then(|g| g.active_pinch.take());
                if let Some(active) = active {
                    if let Some(window) = state.windows.get_mut().get(&active.window_id) {
                        let mut win = window.lock().unwrap();
                        let phase = if cancelled != 0 {
                            GesturePhase::Cancelled(CancellationOrigin::CompositorProtocol)
                        } else {
                            GesturePhase::Ended
                        };
                        win.gesture_buffer.push(WaylandPointerGesture::Pinch {
                            phase,
                            fingers: active.fingers,
                            delta: (0.0, 0.0),
                            scale: active.last_scale,
                            rotation_degrees: 0.0,
                        });

                        if active.fingers == 2 {
                            let touch_phase = if cancelled != 0 {
                                TouchPhase::Cancelled
                            } else {
                                TouchPhase::Ended
                            };
                            let device_id =
                                crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                                    crate::platform_impl::wayland::DeviceId,
                                ));
                            state.events_sink.push_window_event(
                                WindowEvent::PinchGesture {
                                    device_id,
                                    delta: 0.0,
                                    phase: touch_phase,
                                },
                                active.window_id,
                            );
                        }

                        if let Some(requests) =
                            state.window_requests.borrow().get(&active.window_id)
                        {
                            requests.redraw_requested.store(true, Ordering::Relaxed);
                        }
                        state
                            .events_sink
                            .push_window_event(WindowEvent::RedrawRequested, active.window_id);
                    }
                }
            },
            _ => {},
        }
    }
}

delegate_dispatch!(WinitState: [ZwpPointerGesturesV1: GlobalData] => PointerGesturesState);
delegate_dispatch!(WinitState: [ZwpPointerGestureSwipeV1: GlobalData] => PointerGesturesState);
delegate_dispatch!(WinitState: [ZwpPointerGesturePinchV1: GlobalData] => PointerGesturesState);
