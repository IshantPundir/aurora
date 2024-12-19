use std::{
    collections::HashMap,
    os::unix::io::OwnedFd,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use smithay::{
    backend::renderer::element::{default_primary_scanout_output_compare, utils::select_dmabuf_feedback, RenderElementStates},
    delegate_compositor, delegate_data_control, delegate_data_device, delegate_fractional_scale,
    delegate_input_method_manager, delegate_keyboard_shortcuts_inhibit, delegate_layer_shell,
    delegate_output, delegate_pointer_constraints, delegate_pointer_gestures, delegate_presentation,
    delegate_primary_selection, delegate_relative_pointer, delegate_seat, delegate_security_context,
    delegate_shm, delegate_text_input_manager, delegate_viewporter,
    delegate_virtual_keyboard_manager, delegate_xdg_activation, delegate_xdg_decoration, delegate_xdg_shell,
    delegate_xdg_foreign, delegate_single_pixel_buffer, delegate_fifo, delegate_commit_timing,
    desktop::{
        utils::{
            surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
            update_surface_primary_scanout_output, OutputPresentationFeedback,
        },
        PopupKind, PopupManager, Space,
    },
    input::{
        keyboard::{LedState, XkbConfig},
        pointer::PointerHandle,
        Seat, SeatHandler, SeatState,
    },
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, Mode},
        wayland_protocols::xdg::decoration::{
            self as xdg_decoration, zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
        },
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_data_source::WlDataSource, wl_surface::WlSurface},
            Client, Display, DisplayHandle, Resource,
        },
    },
    utils::{Clock, Logical, Monotonic, Rectangle, Time},
    wayland::{
        commit_timing::{CommitTimerBarrierStateUserData, CommitTimingManagerState},
        compositor::{get_parent, with_states, CompositorClientState, CompositorHandler, CompositorState},
        dmabuf::DmabufFeedback,
        fifo::{FifoBarrierCachedState, FifoManagerState},
        fractional_scale::{with_fractional_scale, FractionalScaleHandler, FractionalScaleManagerState},
        input_method::{InputMethodHandler, PopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
        },
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::PointerConstraintsHandler,
        presentation::PresentationState,
        seat::WaylandFocus,
        security_context::{
            SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
        },
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            primary_selection::{set_primary_focus, PrimarySelectionHandler, PrimarySelectionState},
            wlr_data_control::{DataControlHandler, DataControlState},
            SelectionHandler,
        },
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{
                decoration::{XdgDecorationHandler, XdgDecorationState},
                ToplevelSurface, XdgShellState,
            },
        },
        shm::{ShmHandler, ShmState},
        single_pixel_buffer::SinglePixelBufferState,
        socket::ListeningSocketSource,
        viewporter::ViewporterState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
        xdg_foreign::{XdgForeignHandler, XdgForeignState},
    },
};


use crate::{focus::{KeyboardFocusTarget, PointerFocusTarget}, shell::WindowElement, window_manager::WindowManager};

#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<SecurityContext>
}

impl ClientData for ClientState {
    /// Notification that a client was initialized
    fn initialized(&self, _client_id: ClientId) {}
    /// Notification that a client is disconnected
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}    
}

pub trait Backend {
    const HAS_RELATIVE_MOTION: bool = false;
    const HAS_GESTURES: bool = false;
    fn seat_name(&self) -> String;
    fn reset_buffers(&mut self, output: &Output);
    fn early_import(&mut self, surface: &WlSurface);
    fn update_led_state(&mut self, led_state: LedState);
}

#[derive(Debug)]
pub struct AuroraState<BackendData: Backend + 'static> {
    pub backend_data: BackendData,
    pub socket_name: Option<String>,
    pub display_handle: DisplayHandle,
    pub handle: LoopHandle<'static, AuroraState<BackendData>>,
    pub running: Arc<AtomicBool>,
    pub clock: Clock<Monotonic>,

    // desktop
    pub space: Space<WindowElement>,
    pub popups: PopupManager,

    // smithay state
    pub compositor_state: CompositorState,
    pub data_device_state: DataDeviceState,
    pub layer_shell_state: WlrLayerShellState,
    pub output_manager_state: OutputManagerState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub seat_state: SeatState<AuroraState<BackendData>>,
    pub keyboard_shortcuts_inhibit_state: KeyboardShortcutsInhibitState,
    pub shm_state: ShmState,
    pub viewporter_state: ViewporterState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_activation_state: XdgActivationState,
    pub xdg_decoration_state: XdgDecorationState,
    pub xdg_foreign_state: XdgForeignState,
    pub presentation_state: PresentationState,
    pub fractional_scale_manager_state: FractionalScaleManagerState,
    pub single_pixel_buffer_state: SinglePixelBufferState,
    pub fifo_manager_state: FifoManagerState,
    pub commit_timing_manager_state: CommitTimingManagerState,

    // drawing logic???
    pub show_window_preview: bool,

    // input-related fields
    pub seat: Seat<AuroraState<BackendData>>,
    pub seat_name: String,
    pub pointer: PointerHandle<AuroraState<BackendData>>,

    // apps...
    pub window_manager: WindowManager,
}
/*
Delegates the Wayland compositor role to the AuroraState.
This allows Aurora to act as a compositor, managing the display surface, 
organizing client windows, and handling rendering logic.
*/
delegate_compositor!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles events related to outputs (monitor/screens) in the compositor.
Essential to detect new monitors being connected/removed  or repositioned.
Notify Wayland clients of changes in display layout and resolution.
*/
impl <BackendData: Backend> OutputHandler for AuroraState<BackendData> {}
/*
Delegates the output role to the AuroraState.
Handles the output/display management, such as configuring monitors, 
handling screen resolution, and managing multiple screens.
*/
delegate_output!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles events related to shared memory (Shm) buffers which are used to send pixel data
from Wayland clients to the compositor.
*/
impl <BackendData: Backend> ShmHandler for AuroraState<BackendData> {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
/*
Delegates the SHM (shared memory) role to the AuroraState.
Allows clients to share memory buffers with the compositor for rendering.
*/
delegate_shm!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles input device data, such as keyboard and mouse events, and provides
clipboard functionality. It allows clients to receive input from devices like
mice, keyboards, and touchscreens. Without this, clients can't receive input events.
*/
impl <BackendData: Backend> DataDeviceHandler for AuroraState<BackendData> {
    fn data_device_state(&self) -> &smithay::wayland::selection::data_device::DataDeviceState {
        &self.data_device_state
    }
}

/*
Handles **seat events**, which manage input devices (like keyboards, mice, and touchscreens) 
within a logical container called a "seat." The seat tracks which client has input focus 
and controls input devices associated with it. Without this, the compositor would have no 
way to manage input focus or multi-user input.
*/
impl <BackendData: Backend> SeatHandler for AuroraState<BackendData> {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = PointerFocusTarget;
    type TouchFocus = PointerFocusTarget;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &smithay::input::Seat<Self>, target: Option<&Self::KeyboardFocus>) {
        let dh = &self.display_handle;
        let wl_surface = target.and_then(WaylandFocus::wl_surface);
        let focus = wl_surface.and_then(|s| dh.get_client(s.id()).ok());
        
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }

    fn cursor_image(&mut self, _seat: &smithay::input::Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) { }
    
    fn led_state_changed(&mut self, _seat: &smithay::input::Seat<Self>, _led_state: LedState) { }
}
/*
Delegates the seat role to the AuroraState.
Handles input devices (keyboard, mouse, touch) and input events like clicks, touches, and keystrokes.
*/
delegate_seat!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **Client-side Drag-and-Drop** events
*/
impl<BackendData: Backend> ClientDndGrabHandler for AuroraState<BackendData> {
    fn started(&mut self, _source: Option<WlDataSource>, _icon: Option<WlSurface>, _seat: smithay::input::Seat<Self>) {}
    fn dropped(&mut self, _target: Option<WlSurface>, _validated: bool, _seat: smithay::input::Seat<Self>) { }
}

/*
Handles **Server-side Drag-and-Drop** events
Not implementing since this is less common but is required to support drag-and-drop initiated by the compositor.
*/
impl<BackendData: Backend> ServerDndGrabHandler for AuroraState<BackendData> {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: smithay::input::Seat<Self>) {
        unreachable!("Aurora doesn't do server-side grabs");
    }
}
/*
Delegates the data device role to the AuroraState.
Provides support for clipboard and drag-and-drop functionality between Wayland clients.
*/
delegate_data_device!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **selection events** for clipboard functionality. This allows the compositor to 
handle copy-paste operations and manage the selection buffer. Whenever a user copies 
something in a client app, it is stored in the compositor's selection state,
and this handler provides the logic for handling that data.
*/
impl<BackendData: Backend> SelectionHandler for AuroraState<BackendData> {
    type SelectionUserData = ();
}

/*
Handles **data control** for clipboard and data transfer between Wayland clients. 
This allows applications to **copy and paste** (like copying text, images, or files 
from one app to another) and supports drag-and-drop actions. Without this handler, 
clipboard functionality (like Ctrl+C, Ctrl+V) would not work, and apps wouldn't be 
able to share data with each other.
*/
impl<BackendData: Backend> DataControlHandler for AuroraState<BackendData> {
    fn data_control_state(&self) -> &DataControlState {
        &self.data_control_state
    }
}
/*
* Delegates the data control role to the AuroraState.
* Manages clipboard access and data transfer security, controlling what data apps can access from the clipboard.
*/
delegate_data_control!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Handles **primary selection** events, which is a special type of clipboard functionality 
commonly seen in X11, where selecting (highlighting) text automatically copies it to a selection buffer.
*/
impl<BackendData: Backend> PrimarySelectionHandler for AuroraState<BackendData> {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
/*
Delegates the primary selection role to the AuroraState.
Enables "middle-click paste" support on Linux, a legacy selection mechanism often used for quick copy-paste.
*/
delegate_primary_selection!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles events related to tablet input devices. 
Adds support fpr pen pressure, tilt, and touch input from tablets.
NOT REQUIRED FOR AURORA.
*/
// impl<BackendData: Backend> TabletSeatHandler for AuroraState<BackendData> {
//     fn tablet_tool_image(&mut self, _tool: &TabletToolDescriptor, _image: CursorImageStatus) {
//         unreachable!("Drawing tablet is not supported in Aurora.");
//     }
// }
/*
Delegates the tablet manager role to the AuroraState.
Supports Wacom and other graphic tablet devices, handling input from stylus, pen, and tablet touch.
*/
// delegate_tablet_manager!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Delegates the text input manager role to the AuroraState.
Provides support for on-screen keyboards and input methods, which are essential for touchscreen devices.
*/
delegate_text_input_manager!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Handles **input method (IM) support**, such as on-screen keyboards and text input 
for non-keyboard input methods. This is crucial for devices like touchscreens, 
where users rely on on-screen keyboards for typing. Without this, users can't type 
in applications if they don't have access to a physical keyboard.
*/
impl<BackendData: Backend> InputMethodHandler for AuroraState<BackendData> {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("Failed to track popup: {}", err);
        }
    }

    fn popup_repositioned(&mut self, _: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, smithay::utils::Logical> {
        self.space
            .elements()
            .find_map(|window| (window.wl_surface().as_deref() == Some(parent)).then(|| window.0.geometry()))
            .unwrap_or_default()
    }
}
/*
Delegates the input method manager role to the AuroraState.
Manages on-screen input methods for text input, like virtual keyboards and text prediction tools.
*/
delegate_input_method_manager!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles the ability to **inhibit global keyboard shortcuts**
Without this, global shortcuts would always trigger, disrupting fullscreen experiences.
*/
impl<BackendData: Backend> KeyboardShortcutsInhibitHandler for AuroraState<BackendData> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // Just grant the wish for everyone
        inhibitor.activate();
    }
}

/*
Delegates the keyboard shortcuts inhibit role to the AuroraState.
Prevents specific keyboard shortcuts from being intercepted by the compositor, allowing apps to override them.
*/
delegate_keyboard_shortcuts_inhibit!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Delegates the virtual keyboard manager role to the AuroraState.
Manages the display and interaction of on-screen virtual keyboards.
*/
delegate_virtual_keyboard_manager!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates pointer gestures to the AuroraState.
Supports touchpad gestures like pinch-to-zoom and two-finger scrolling.
*/
delegate_pointer_gestures!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Delegates relative pointer role to the AuroraState.
Handles relative input from devices like mice, especially useful in gaming where cursor position is relative, not absolute.
*/
delegate_relative_pointer!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Handles **pointer constraints**, which restrict how and where the mouse pointer can move.
Not needed, since Aurora is touch-screen based compositor.
*/
impl<BackendData: Backend> PointerConstraintsHandler for AuroraState<BackendData> {
    fn new_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {}

    fn cursor_position_hint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _location: smithay::utils::Point<f64, Logical>,
    ) { }
}
/*
Delegates pointer constraints to the AuroraState.
Restricts pointer movements, useful for applications like games that lock the cursor inside a window.
*/
delegate_pointer_constraints!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Delegates the viewporter role to the AuroraState.
Allows applications to crop and scale their surfaces to fit specific viewports, useful for games and video players.
*/
delegate_viewporter!(@<BackendData: Backend + 'static> AuroraState<BackendData>);
/*
Handles **XDG activation**, which allows apps to request focus (like when you click
on a notification, and the corresponding window is activated).
It manages app activation tokens, letting apps tell the compositor that they should be given focus. 
This is critical for notification click-to-focus and for apps requesting attention.
*/
impl<BackendData: Backend> XdgActivationHandler for AuroraState<BackendData> {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        if let Some((serial, seat)) = data.serial {
            let keyboard = self.seat.get_keyboard().unwrap();
            Seat::from_resource(&seat) == Some(self.seat.clone())
                && keyboard
                    .last_enter()
                    .map(|last_enter| serial.is_no_older_than(&last_enter))
                    .unwrap_or(false)
        } else {
            false
        }
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if token_data.timestamp.elapsed().as_secs() < 10 {
            // Just grant the wish
            let w = self
                .space
                .elements()
                .find(|window| window.wl_surface().map(|s| *s == surface).unwrap_or(false))
                .cloned();
            if let Some(window) = w {
                self.space.raise_element(&window, true);
            }
        }
    }
}
/*
Delegates the XDG activation role to the AuroraState.
Handles activation requests from Wayland clients, allowing apps to request user attention (like opening a window).
*/
delegate_xdg_activation!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **XDG window decoration** requests. XDG surfaces (like XDG-toplevel windows) 
allow clients to request window decorations (title bar, close/minimize buttons, etc.). 
This handler lets Aurora control how window decorations are drawn.
This is essential for standardize how app windows appear in your compositor.
*/
impl<BackendData: Backend> XdgDecorationHandler for AuroraState<BackendData> {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        // Set the default to client side
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(match mode {
                DecorationMode::ServerSide => Mode::ServerSide,
                _ => Mode::ClientSide,
            });
        });

        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        use xdg_decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });

        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

/*
Delegates the XDG decoration role to the AuroraState.
Provides support for server-side decorations (like window borders, shadows, etc.) used in modern desktop environments.
*/
delegate_xdg_decoration!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the XDG shell role to the AuroraState.
Manages application windows (like maximizing, minimizing, and fullscreening) in Wayland.
*/
delegate_xdg_shell!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the layer shell role to the AuroraState.
Supports layer-based surfaces like panels, lock screens, and status bars.
*/
delegate_layer_shell!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the presentation role to the AuroraState.
Manages frame presentation timing, enabling smooth animations and frame rate synchronization.
*/
delegate_presentation!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **fractional scaling** support, which allows high-DPI (HiDPI) displays to 
use fractional display scales (like 1.25x, 1.5x) instead of just integers (1x, 2x).
Without this, text and UI elements may look blurry or scaled incorrectly on high-DPI screens.
*/
impl<BackendData: Backend> FractionalScaleHandler for AuroraState<BackendData> {
    fn new_fractional_scale(
        &mut self,
        surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        // Here we can set the initial fractional scale
        //
        // First we look if the surface already has a primary scan-out output, if not
        // we test if the surface is a subsurface and try to use the primary scan-out output
        // of the root surface. If the root also has no primary scan-out output we just try
        // to use the first output of the toplevel.
        // If the surface is the root we also try to use the first output of the toplevel.
        //
        // If all the above tests do not lead to a output we just use the first output
        // of the space (which in case of anvil will also be the output a toplevel will
        // initially be placed on)
        #[allow(clippy::redundant_clone)]
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }

        with_states(&surface, |states| {
            let primary_scanout_output = surface_primary_scanout_output(&surface, states)
                .or_else(|| {
                    if root != surface {
                        with_states(&root, |states| {
                            surface_primary_scanout_output(&root, states).or_else(|| {
                                self.window_for_surface(&root).and_then(|window| {
                                    self.space.outputs_for_element(&window).first().cloned()
                                })
                            })
                        })
                    } else {
                        self.window_for_surface(&root)
                            .and_then(|window| self.space.outputs_for_element(&window).first().cloned())
                    }
                })
                .or_else(|| self.space.outputs().next().cloned());
            if let Some(output) = primary_scanout_output {
                with_fractional_scale(states, |fractional_scale| {
                    fractional_scale.set_preferred_scale(output.current_scale().fractional_scale());
                });
            }
        });
    }
}
/*
Delegates the fractional scale role to the AuroraState.
Provides fractional scaling support (e.g., 150% display scale) for high-DPI displays.
*/
delegate_fractional_scale!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **security context checks** within the compositor.
This allows Aurora to  enforce security policies for Wayland clients, such as ensuring certain operations
(like SOUL Integration) are only allowed for trusted apps. 
This is crucial for privacy and security since it prevents malicious apps from capturing the screen or input without user consent.
*/
impl<BackendData: Backend + 'static> SecurityContextHandler for AuroraState<BackendData> {
    fn context_created(&mut self, source: SecurityContextListenerSource, security_context: SecurityContext) {
        self.handle
            .insert_source(source, move |client_stream, _, data| {
                let client_state = ClientState {
                    security_context: Some(security_context.clone()),
                    ..ClientState::default()
                };
                if let Err(err) = data
                    .display_handle
                    .insert_client(client_stream, Arc::new(client_state))
                {
                    tracing::warn!("Error adding wayland client: {}", err);
                };
            })
            .expect("Failed to init wayland socket source");
    }
}
/*
Delegates the security context role to the AuroraState.
Provides security context for client connections, defining permissions and isolation between apps.
*/
delegate_security_context!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Handles **XDG Foreign support**, which allows Wayland clients to export their 
windows and give other clients a way to reference them. This enables features 
like embedding one app's window into another (like embedding a video player in a 
browser tab or embedding a widget from one app into another). This is essential 
for advanced multi-window workflows, window embedding, and inter-process communication 
(IPC) between Wayland clients.
*/
impl<BackendData: Backend> XdgForeignHandler for AuroraState<BackendData> {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}

/*
Delegates the XDG foreign role to the AuroraState.
Allows sharing of windows and surfaces between Wayland clients, useful for app embedding.
*/
delegate_xdg_foreign!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the single pixel buffer role to the AuroraState.
Optimizes single-color surfaces by reducing memory usage, used for backgrounds and placeholders.
*/
delegate_single_pixel_buffer!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the FIFO role to the AuroraState.
Manages frame scheduling and synchronization, ensuring efficient frame delivery.
*/
delegate_fifo!(@<BackendData: Backend + 'static> AuroraState<BackendData>);

/*
Delegates the commit timing role to the AuroraState.
Allows clients to request precise frame commit timings, optimizing frame presentation for smoother animations.
*/
delegate_commit_timing!(@<BackendData: Backend + 'static> AuroraState<BackendData>);


#[derive(Debug, Clone)]
pub struct SurfaceDmabufFeedback {
    pub render_feedback: DmabufFeedback,
    pub scanout_feedback: DmabufFeedback,
}

impl<BackendData: Backend + 'static> AuroraState<BackendData> {
    
}

pub fn take_presentation_feedback(
    output: &Output,
    space: &Space<WindowElement>,
    render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    let mut output_presentation_feedback = OutputPresentationFeedback::new(output);

    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut output_presentation_feedback,
                surface_primary_scanout_output,
                |surface, _| surface_presentation_feedback_flags_from_states(surface, render_element_states),
            );
        }
    });
    let map = smithay::desktop::layer_map_for_output(output);
    for layer_surface in map.layers() {
        layer_surface.take_presentation_feedback(
            &mut output_presentation_feedback,
            surface_primary_scanout_output,
            |surface, _| surface_presentation_feedback_flags_from_states(surface, render_element_states),
        );
    }

    output_presentation_feedback
}

impl <BackendData: Backend + 'static> AuroraState<BackendData> {
    pub fn init(
        display: Display<AuroraState<BackendData>>,
        handle: LoopHandle<'static, AuroraState<BackendData>>,
        backend_data: BackendData,
        listen_on_socket: bool,
    ) -> AuroraState<BackendData> {
        let dh = display.handle();

        let socket_name = if listen_on_socket {
            let source = ListeningSocketSource::new_auto().unwrap();
            let socket_name = source.socket_name().to_string_lossy().into_owned();
            handle
                .insert_source(source, |client_stream, _, data| {
                    if let Err(err) = data
                        .display_handle
                        .insert_client(client_stream, Arc::new(ClientState::default()))
                        {
                            tracing::warn!("Error adding wayland client: {}", err);
                        };
                }).expect("Failed to init wayland socket source");
            
            tracing::info!(name=socket_name, "Listening on wayland socket");
            Some(socket_name)
        } else { None };

        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, data| {
                    unsafe {
                        display.get_mut().dispatch_clients(data).unwrap();
                    }
                    Ok(smithay::reexports::calloop::PostAction::Continue)
                },
            ).expect("Failed to init wayland server source");
        /* init globals*/
        // Manages wayland compositor logic for wl_surface objects.
        let compositor_state = CompositorState::new::<Self>(&dh);
        // Implements data device protocol (wl_data_device), allowing clients to support drag-and-drop and clipboard support.
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        // Implements the wlr-layer-shell protocol. layer shell windows sit below or above normal windows.
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        // Manages Wayland outputs (display/monitors) & interact with xdg-output protocol.
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        // Implements the primary selection protocol. This protocol allows selecting and copying data without explicit
        // copy-paste actions (like selecting text in X11 and pasting it with the middle mouse button).
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        // Implements wl-data-control protocol, which allows applications (like a clipboard manager) to interact with the clipboard.
        let data_control_state = DataControlState::new::<Self, _>(&dh, Some(&primary_selection_state), |_| true);
        // Represents input devices like Keyboards, mics & touchscreens.
        let mut seat_state = SeatState::new();
        // Implements the shared memory protocol, allowing clients to use shared memory for drawing buffer.
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        // Implements the viewport protocol, allowing  clients to crop and scale their surfaces.
        // Use Case: Required for clients to crop/scale surfaces. Useful for rendering thumbnails, previews, or scaled views of content.
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        // Implements the xdg-shell protocol, providing high-level abstraction for toplevel (window) & popups.
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        // Handles xdg-activation-v1, which allows activating application windows on user request.
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        // Implements the xdg-decoration protocol, allowing clients to request server-side or client-side window decoration.
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        // Allows one application to export surfaces and for another to import them
        let xdg_foreign_state = XdgForeignState::new::<Self>(&dh);
        // Implements the presentation-time protocol, allowing clients to synchronize their frame presentation with the compositor.
        let clock = Clock::new();
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);
        // Handles fractional scaling of application surface. so clients can render at non-integer scale (like 150% inseted of 100%)
        let fractional_scale_manager_state = FractionalScaleManagerState::new::<Self>(&dh);

        // Allows creation of a single-pixel buffer that can be stretched to any size.
        let single_pixel_buffer_state = SinglePixelBufferState::new::<Self>(&dh);
        // Manages frame synchronization (FIFO-based frame presentation)
        let fifo_manager_state = FifoManagerState::new::<Self>(&dh);
        // Tracks the timing of surface commits.
        let commit_timing_manager_state = CommitTimingManagerState::new::<Self>(&dh);
        VirtualKeyboardManagerState::new::<Self, _>(&dh, |_client| true);

        /* Init inputs*/
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());
        let pointer = seat.add_pointer();
        seat.add_keyboard(XkbConfig::default(), 200, 25)
            .expect("Failed to initialize the keyboard");
        let keyboard_shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);

        AuroraState {
            backend_data,
            socket_name,
            display_handle: dh,
            handle,
            running: Arc::new(AtomicBool::new(true)),
            clock,

            space: Space::default(),
            popups: PopupManager::default(),
            
            compositor_state,
            data_device_state,
            layer_shell_state,
            output_manager_state,
            primary_selection_state,
            data_control_state,
            seat_state,
            keyboard_shortcuts_inhibit_state,
            shm_state,
            viewporter_state,
            xdg_shell_state,
            xdg_activation_state,
            xdg_decoration_state,
            xdg_foreign_state,
            presentation_state,
            fractional_scale_manager_state,
            single_pixel_buffer_state,
            fifo_manager_state,
            commit_timing_manager_state,

            show_window_preview: false,

            seat,
            seat_name,
            pointer,

            window_manager: WindowManager::new()
        }
    }

    pub fn pre_repaint(&mut self, output: &Output, frame_target: impl Into<Time<Monotonic>>) {
        let frame_target = frame_target.into();

        #[allow(clippy::mutable_key_type)]
        let mut clients: HashMap<ClientId, Client> = HashMap::new();
        self.space.elements().for_each(|window| {
            window.with_surfaces(|surface, states| {
                let clear_commit_timer = surface_primary_scanout_output(surface, states)
                    .map(|primary_output| &primary_output == output)
                    .unwrap_or(true);

                if clear_commit_timer {
                    if let Some(mut commit_timer_state) = states
                        .data_map
                        .get::<CommitTimerBarrierStateUserData>()
                        .map(|commit_timer| commit_timer.lock().unwrap())
                    {
                        commit_timer_state.signal_until(frame_target);
                        let client = surface.client().unwrap();
                        clients.insert(client.id(), client);
                    }
                }
            });
        });

        let map = smithay::desktop::layer_map_for_output(output);
        for layer_surface in map.layers() {
            layer_surface.with_surfaces(|surface, states| {
                let clear_commit_timer = surface_primary_scanout_output(surface, states)
                    .map(|primary_output| &primary_output == output)
                    .unwrap_or(true);

                if clear_commit_timer {
                    if let Some(mut commit_timer_state) = states
                        .data_map
                        .get::<CommitTimerBarrierStateUserData>()
                        .map(|commit_timer| commit_timer.lock().unwrap())
                    {
                        commit_timer_state.signal_until(frame_target);
                        let client = surface.client().unwrap();
                        clients.insert(client.id(), client);
                    }
                }
            });
        }

        let dh = self.display_handle.clone();
        for client in clients.into_values() {
            self.client_compositor_state(&client).blocker_cleared(self, &dh);
        }
    }

    pub fn post_repaint(
        &mut self,
        output: &Output,
        time: impl Into<Duration>,
        dmabuf_feedback: Option<SurfaceDmabufFeedback>,
        render_element_states: &RenderElementStates,
    ) {
        let time = time.into();
        let throttle = Some(Duration::from_secs(1));

        #[allow(clippy::mutable_key_type)]
        let mut clients: HashMap<ClientId, Client> = HashMap::new();

        self.space.elements().for_each(|window| {
            window.with_surfaces(|surface, states| {
                let primary_scanout_output = update_surface_primary_scanout_output(
                    surface,
                    output,
                    states,
                    render_element_states,
                    default_primary_scanout_output_compare,
                );

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale.set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        let client = surface.client().unwrap();
                        clients.insert(client.id(), client);
                    }
                }
            });

            if self.space.outputs_for_element(window).contains(output) {
                window.send_frame(output, time, throttle, surface_primary_scanout_output);
                if let Some(dmabuf_feedback) = dmabuf_feedback.as_ref() {
                    window.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
                        select_dmabuf_feedback(
                            surface,
                            render_element_states,
                            &dmabuf_feedback.render_feedback,
                            &dmabuf_feedback.scanout_feedback,
                        )
                    });
                }
            }
        });
        let map = smithay::desktop::layer_map_for_output(output);
        for layer_surface in map.layers() {
            layer_surface.with_surfaces(|surface, states| {
                let primary_scanout_output = update_surface_primary_scanout_output(
                    surface,
                    output,
                    states,
                    render_element_states,
                    default_primary_scanout_output_compare,
                );

                if let Some(output) = primary_scanout_output.as_ref() {
                    with_fractional_scale(states, |fraction_scale| {
                        fraction_scale.set_preferred_scale(output.current_scale().fractional_scale());
                    });
                }

                if primary_scanout_output
                    .as_ref()
                    .map(|o| o == output)
                    .unwrap_or(true)
                {
                    let fifo_barrier = states
                        .cached_state
                        .get::<FifoBarrierCachedState>()
                        .current()
                        .barrier
                        .take();

                    if let Some(fifo_barrier) = fifo_barrier {
                        fifo_barrier.signal();
                        let client = surface.client().unwrap();
                        clients.insert(client.id(), client);
                    }
                }
            });

            layer_surface.send_frame(output, time, throttle, surface_primary_scanout_output);
            if let Some(dmabuf_feedback) = dmabuf_feedback.as_ref() {
                layer_surface.send_dmabuf_feedback(output, surface_primary_scanout_output, |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &dmabuf_feedback.render_feedback,
                        &dmabuf_feedback.scanout_feedback,
                    )
                });
            }
        }

        let dh = self.display_handle.clone();
        for client in clients.into_values() {
            self.client_compositor_state(&client).blocker_cleared(self, &dh);
        }
    }
}
