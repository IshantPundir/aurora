use std::{sync::atomic::Ordering, time::Duration};

use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        egl::EGLDevice,
        renderer::{
            damage::{Error as OutputDamageTrackerError, OutputDamageTracker}, gles::GlesRenderer, ImportDma, ImportEgl, ImportMemWl
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
        SwapBuffersError,
    }, delegate_dmabuf, input::keyboard::LedState,
        output::{Mode, Output, PhysicalProperties, Subpixel}, reexports::{
        calloop::EventLoop,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::{protocol::wl_surface, Display},
        winit::platform::pump_events::PumpStatus,
    }, utils::Transform, wayland::{
        dmabuf::{
            DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
        },
        presentation::Refresh,
    }
};


use crate::{renderer::{render_output, CustomRenderElements}, state::{take_presentation_feedback, AuroraState, Backend}};

pub const OUTPUT_NAME: &str = "winit";

pub struct WinitData {
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    full_redraw: u8
}

impl DmabufHandler for AuroraState<WinitData> {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.backend_data.dmabuf_state.0
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        if self
            .backend_data
            .backend
            .renderer()
            .import_dmabuf(&dmabuf, None)
            .is_ok()
        {
            let _ = notifier.successful::<AuroraState<WinitData>>();
        } else {
            notifier.failed();
        }
    }
}
delegate_dmabuf!(AuroraState<WinitData>);

impl Backend for WinitData {
    fn seat_name(&self) -> String {
        String::from("winit")
    }
    fn reset_buffers(&mut self, _output: &Output) {
        self.full_redraw = 4;
    }
    fn early_import(&mut self, _surface: &wl_surface::WlSurface) {}
    fn update_led_state(&mut self, _led_state: LedState) {}
}



pub fn run_winit() {
    tracing::info!("Running with winit backend");
    tracing::warn!("Only for debuging and development porpose");
    // Initialization
    let mut event_loop = EventLoop::try_new().unwrap(); // manages system events like input, resize & app-specific events.
    let display = Display::new().unwrap(); // Wayland protocol Display, managing Wayland client & protocol interactions.
    let mut display_handle = display.handle(); // handle to interact with the Wayland display.

    // Backend initialization
    #[cfg_attr(not(feature = "egl"), allow(unused_mut))]
    let (mut backend, mut winit) = match winit::init::<GlesRenderer>() {
        Ok(ret) => ret,
        Err(err) => {
            tracing::error!("Failed to initialize Winit backend: {}", err);
            return;
        }
    };

    // Output setup
    let size = backend.window_size();
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    // output represents a Wayland output (eg: a monitor)
    let output = Output::new(
        OUTPUT_NAME.to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );

    // Output configuration
    let _global = output.create_global::<AuroraState<WinitData>>(&display.handle());
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);


    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    // DMA-BUF Support
    // for sharing bufferrs (eg. textures) between components (eg. GPU & compositor)
    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats();
            let dmabuf_default_feedback = DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                .build()
                .unwrap();
            Some(dmabuf_default_feedback)
        }
        Ok(None) => {
            tracing::warn!("failed to query render node, dmabuf will use v3");
            None
        }
        Err(err) => {
            tracing::warn!(?err, "failed to egl device for display, dmabuf will use v3");
            None
        }
    };

    // if we failed to build dmabuf feedback we fall back to dmabuf v3
    // Note: egl on Mesa requires either v4 or wl_drm (initialized with bind_wl_display)
    let dmabuf_state = if let Some(default_feedback) = dmabuf_default_feedback {
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<AuroraState<WinitData>>(
            &display.handle(),
            &default_feedback,
        );
        (dmabuf_state, dmabuf_global, Some(default_feedback))
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats();
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global =
            dmabuf_state.create_global::<AuroraState<WinitData>>(&display.handle(), dmabuf_formats);
        (dmabuf_state, dmabuf_global, None)
    };

    #[cfg(feature = "egl")]
    if backend.renderer().bind_wl_display(&display.handle()).is_ok() {
        tracing::info!("EGL hardware-acceleration enabled");
    };

    // State and Data initialization
    let data = {
        // It tracks which part of the output need redrawing
        let damage_tracker = OutputDamageTracker::from_output(&output);

        WinitData {
            backend,
            damage_tracker,
            dmabuf_state,
            full_redraw: 0,
        }
    };

    // Aurora state object managin compositor-specific data.(eg.surfaces, inputs).
    let mut state = AuroraState::init(display, event_loop.handle(), data, true);
    

    state
        .shm_state
        .update_formats(state.backend_data.backend.renderer().shm_formats());
    state.space.map_output(&output, (0, 0));

    tracing::info!("Initialization completed, starting the main loop.");

    while state.running.load(Ordering::SeqCst) {
        // Event handling

        // input and resizing
        let status = winit.dispatch_new_events(|event| match event {
            // Updates output mode & repositions content when window is resized
            WinitEvent::Resized  { size, .. }  => {
                let output = state.space.outputs().next().unwrap().clone();
                state.space.map_output(&output, (0, 0));

                let mode = Mode {
                    size,
                    refresh: 60_000,
                };
                output.change_current_state(Some(mode), None, None, None);
                output.set_preferred(mode);
                crate::shell::fixup_positions(&mut state.space, &mut state.window_manager, state.pointer.current_location());
            }

            WinitEvent::Input(event) => state.process_input_event_windowed(event, OUTPUT_NAME),
            
            _ => (),
        });

        if let PumpStatus::Exit(_) = status {
            state.running.store(false, Ordering::SeqCst);
            break;
        }

        // drawing logic
        {
            let now = state.clock.now();
            let frame_target = now
                + output
                    .current_mode()
                    .map(|mode| Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
                    .unwrap_or_default();
            state.pre_repaint(&output, frame_target);

            let backend = &mut state.backend_data.backend;

            let full_redraw = &mut state.backend_data.full_redraw;
            *full_redraw = full_redraw.saturating_sub(1);
            
            let space = &mut state.space;
            let damage_tracker = &mut state.backend_data.damage_tracker;
            let show_window_preview = state.show_window_preview;


            // Binds the rendering backend to start a new frame. This prepares the rendering 
            // target, such as framebuffer or output surface.
            let render_res = backend.bind().and_then(|_| {
                // Determine whether a full redraw is needed or if partial updates (damage tracking) are sufficiant.

                // If full redraw is requested, age is set to 0. This ensures the compositor redraw everything.
                let age = if *full_redraw > 0 {
                    0
                } else {
                    backend.buffer_age().unwrap_or(0)
                };

                let renderer = backend.renderer();
                
                // Creating a list of render elements.
                let elements: Vec<CustomRenderElements<GlesRenderer>> = Vec::<CustomRenderElements<GlesRenderer>>::new();

                // Renders the output, including surfaces, cursors, and other elements.
                render_output(
                    &output,
                    space,
                    elements,
                    renderer,
                    damage_tracker,
                    age,
                    show_window_preview,
                )
                .map_err(|err| match err {
                    OutputDamageTrackerError::Rendering(err) => err.into(),
                    _ => unreachable!(),
                })
                
            });

            match render_res {
                Ok(render_output_result) => {
                    let has_rendered = render_output_result.damage.is_some();
                    if let Some(damage) = render_output_result.damage {
                        if let Err(err) = backend.submit(Some(damage)) {
                            tracing::warn!("Failed to submit buffer: {}", err);
                        }
                    }

                    let states = render_output_result.states;
                    if has_rendered {
                        let mut output_presentation_feedback = take_presentation_feedback(&output, &state.space, &states);
                        
                        output_presentation_feedback.presented(
                            frame_target,
                            output
                                .current_mode()
                                .map(|mode| {
                                    Refresh::fixed(Duration::from_secs_f64(1_000f64 / mode.refresh as f64))
                                })
                                .unwrap_or(Refresh::Unknown),
                            0,
                            wp_presentation_feedback::Kind::Vsync,
                        )
                    }

                    // Send frame events so that client start drawing their next frame
                    state.post_repaint(&output, frame_target, None, &states);
                }
                Err(SwapBuffersError::ContextLost(err)) => {
                    tracing::error!("Critical Rendering Error: {}", err);
                    state.running.store(false, Ordering::SeqCst);
                }
                Err(err) => tracing::warn!("Rendering error: {}", err),
            }
        }

        // Cleanup and Client updates.
        let result = event_loop.dispatch(Some(Duration::from_millis(1)), &mut state);
        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            state.space.refresh();
            state.popups.cleanup();
            display_handle.flush_clients().unwrap();
        }
    }
}
