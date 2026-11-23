//! Standalone diagnostic example for Wayland pointer gestures (swipe & pinch).
//!
//! Validates whether the active Wayland compositor delivers zwp_pointer_gestures_v1
//! to the client window, testing:
//! - 2-finger scroll (wl_pointer.axis -> MouseWheel)
//! - 2-finger pinch (zwp_pointer_gesture_pinch_v1 -> Pinch with cumulative scale)
//! - 4-finger swipe (zwp_pointer_gesture_swipe_v1 -> Swipe with fingers=4)
//! - 3-finger gesture observation (distinguishing compositor workspace capture from client delivery)
//! - Gesture cancellation on cursor leave / focus change / window destruction.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::platform::wayland::{
    CancellationOrigin, GesturePhase, WaylandPointerGesture, WindowExtWayland,
};
use winit::window::{Window, WindowAttributes, WindowId};

struct App {
    window: Option<Arc<Window>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    start_time: Instant,
    last_log_time: Instant,
    update_count: u32,
}

impl App {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            window: None,
            surface: None,
            start_time: now,
            last_log_time: now,
            update_count: 0,
        }
    }

    fn timestamp(&self) -> String {
        let elapsed = self.start_time.elapsed();
        format!("{:02}.{:03}s", elapsed.as_secs(), elapsed.subsec_millis())
    }

    fn draw_frame(&mut self) {
        if let (Some(window), Some(surface)) = (&self.window, &mut self.surface) {
            let size = window.inner_size();
            let width = match NonZeroU32::new(size.width) {
                Some(w) => w,
                None => return,
            };
            let height = match NonZeroU32::new(size.height) {
                Some(h) => h,
                None => return,
            };

            if surface.resize(width, height).is_ok() {
                if let Ok(mut buffer) = surface.buffer_mut() {
                    // Dark drafting background (#1a1d24)
                    buffer.fill(0x1a1d24);
                    let _ = buffer.present();
                }
            }
        }
    }

    fn process_gestures(&mut self) {
        let gestures = if let Some(window) = &self.window {
            window.drain_pointer_gestures()
        } else {
            Vec::new()
        };

        for gesture in gestures {
            match gesture {
                WaylandPointerGesture::Swipe { phase, fingers, delta } => {
                    match phase {
                        GesturePhase::Started => {
                            self.update_count = 0;
                            println!(
                                "[{}] [SWIPE] >>> INICIO (Started) | Dedos: {} | Destino asociado a ventana",
                                self.timestamp(),
                                fingers
                            );
                        }
                        GesturePhase::Updated => {
                            self.update_count += 1;
                            // Throttled logging to keep output readable while proving continuous updates
                            if self.update_count <= 3
                                || self.update_count % 10 == 0
                                || self.last_log_time.elapsed().as_millis() >= 100
                            {
                                println!(
                                    "[{}] [SWIPE] ACTUALIZAR (Updated #{}): dx={:+7.2} pt, dy={:+7.2} pt (incremental)",
                                    self.timestamp(),
                                    self.update_count,
                                    delta.0,
                                    delta.1
                                );
                                self.last_log_time = Instant::now();
                            }
                        }
                        GesturePhase::Ended => {
                            println!(
                                "[{}] [SWIPE] <<< FIN NORMAL (Ended) | Total updates: {} | Dedos: {}",
                                self.timestamp(),
                                self.update_count,
                                fingers
                            );
                        }
                        GesturePhase::Cancelled(origin) => {
                            let origin_str = match origin {
                                CancellationOrigin::CompositorProtocol => {
                                    "Compositor/Protocolo (cancel=1 del servidor Wayland)"
                                }
                                CancellationOrigin::PointerLeave => {
                                    "Parche local ante pointer.leave (cursor abandonó ventana)"
                                }
                                CancellationOrigin::WindowOrDeviceCleanup => {
                                    "Limpieza local (ventana destruida o dispositivo retirado)"
                                }
                            };
                            println!(
                                "[{}] [SWIPE] !!! CANCELADO | Origen: {} | Total updates: {} | Dedos: {}",
                                self.timestamp(),
                                origin_str,
                                self.update_count,
                                fingers
                            );
                        }
                    }
                }
                WaylandPointerGesture::Pinch {
                    phase,
                    fingers,
                    delta,
                    scale,
                    rotation_degrees,
                } => match phase {
                    GesturePhase::Started => {
                        self.update_count = 0;
                        let role = if fingers == 2 {
                            "CANDIDATO ZOOM (2 dedos)"
                        } else {
                            "NO ZOOM (Ignorar para navegación)"
                        };
                        println!(
                            "[{}] [PINCH] >>> INICIO (Started) | Dedos: {} [{}] | Escala inicial: {:.4}x",
                            self.timestamp(),
                            fingers,
                            role,
                            scale
                        );
                    }
                    GesturePhase::Updated => {
                        self.update_count += 1;
                        if self.update_count <= 3
                            || self.update_count % 10 == 0
                            || self.last_log_time.elapsed().as_millis() >= 100
                        {
                            println!(
                                "[{}] [PINCH] ACTUALIZAR (Updated #{} | Dedos: {}): Escala={:.4}x | rot={:+6.2}° | dxy=({:+5.1}, {:+5.1}) pt",
                                self.timestamp(),
                                self.update_count,
                                fingers,
                                scale,
                                rotation_degrees,
                                delta.0,
                                delta.1
                            );
                            self.last_log_time = Instant::now();
                        }
                    }
                    GesturePhase::Ended => {
                        println!(
                            "[{}] [PINCH] <<< FIN NORMAL (Ended) | Total updates: {} | Dedos: {} | Escala final={:.4}x",
                            self.timestamp(),
                            self.update_count,
                            fingers,
                            scale
                        );
                    }
                    GesturePhase::Cancelled(origin) => {
                        let origin_str = match origin {
                            CancellationOrigin::CompositorProtocol => {
                                "Compositor/Protocolo (cancel=1 del servidor Wayland)"
                            }
                            CancellationOrigin::PointerLeave => {
                                "Parche local ante pointer.leave (cursor abandonó ventana)"
                            }
                            CancellationOrigin::WindowOrDeviceCleanup => {
                                "Limpieza local (ventana destruida o dispositivo retirado)"
                            }
                        };
                        println!(
                            "[{}] [PINCH] !!! CANCELADO | Origen: {} | Total updates: {} | Dedos: {} | Escala={:.4}x",
                            self.timestamp(),
                            origin_str,
                            self.update_count,
                            fingers,
                            scale
                        );
                    }
                },
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("axialCAD - Diagnostico de Gestos Wayland (Fase 1)")
            .with_inner_size(LogicalSize::new(800.0, 600.0));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[ERROR] No se pudo crear la ventana winit: {}", e);
                event_loop.exit();
                return;
            }
        };

        let gestures_ok = window.is_pointer_gestures_available();

        println!("================================================================================");
        println!("  axialCAD - DIAGNÓSTICO DE GESTOS WAYLAND (Fase 1 Standalone)");
        println!("================================================================================");
        println!("  Protocolo zwp_pointer_gestures_v1: {}", if gestures_ok { "DISPONIBLE (OK)" } else { "NO DISPONIBLE" });
        println!("  Ciclo de eventos: ControlFlow::Wait (dormido en epoll; despierta al recibir eventos)");
        println!("  Buffer: Bounded 128 eventos, política LIFO de descarte de actualizaciones intermedias");
        println!("--------------------------------------------------------------------------------");
        println!("  GUÍA PARA LA PRUEBA FÍSICA:");
        println!("  1. DOS DEDOS (Scroll): Desliza vertical/horizontalmente.");
        println!("     -> Debe reportar eventos [SCROLL 2-DEDOS] (normal wheel).");
        println!("  2. PINZA (Pinch): Separa o junta 2 dedos.");
        println!("     -> Si el compositor lo entrega: reportará [PINCH] con escala acumulada (1.0 = base).");
        println!("  3. CUATRO DEDOS (Swipe): Desliza 4 dedos horizontal o verticalmente.");
        println!("     -> Si el compositor lo entrega: reportará [SWIPE] con fingers=4.");
        println!("  4. TRES DEDOS: Desliza 3 dedos.");
        println!("     -> Si GNOME Shell captura el gesto para cambiar de escritorio / overview,");
        println!("        la aplicación NO recibirá eventos, lo cual confirma la reserva del entorno.");
        println!("  5. SALIDA / ENTRADA DE CURSOR Y FOCO: Mueve el cursor fuera de la ventana");
        println!("     durante un gesto -> debe cancelar y limpiar el estado de inmediato.");
        println!("  Presiona ESC o cierra la ventana para salir.");
        println!("================================================================================\n");

        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[ERROR] Fallo al crear contexto softbuffer: {}", e);
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ERROR] Fallo al crear superficie softbuffer: {}", e);
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.surface = Some(surface);
        self.draw_frame();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        // Drain any pointer gestures delivered with this event/iteration
        self.process_gestures();

        match event {
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(x, y) => format!("Line(x={:+5.2}, y={:+5.2})", x, y),
                    MouseScrollDelta::PixelDelta(pos) => format!("Pixel(x={:+6.1}, y={:+6.1})", pos.x, pos.y),
                };
                println!("[{}] [SCROLL 2-DEDOS] {} | Phase={:?}", self.timestamp(), d, phase);
            }
            WindowEvent::PinchGesture { delta, phase, .. } => {
                println!(
                    "[{}] [PINCH winit bridge (solo 2 dedos)] delta={:+7.4} (incremental) | Phase={:?}",
                    self.timestamp(),
                    delta,
                    phase
                );
            }
            WindowEvent::CursorEntered { .. } => {
                println!("[{}] [CURSOR] Cursor ha entrado en la superficie de la ventana", self.timestamp());
            }
            WindowEvent::CursorLeft { .. } => {
                println!("[{}] [CURSOR] Cursor ha salido de la superficie (gestos limpiados/cancelados)", self.timestamp());
            }
            WindowEvent::Focused(focused) => {
                println!("[{}] [FOCO] Foco de ventana: {}", self.timestamp(), if focused { "ACTIVO" } else { "INACTIVO" });
            }
            WindowEvent::RedrawRequested => {
                self.draw_frame();
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { state: ElementState::Pressed, logical_key: Key::Named(NamedKey::Escape), .. },
                ..
            }
            | WindowEvent::CloseRequested => {
                println!("[{}] [SALIDA] Cerrando prueba de diagnóstico...", self.timestamp());
                event_loop.exit();
            }
            _ => {}
        }

        // Drain any gestures produced during processing
        self.process_gestures();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
