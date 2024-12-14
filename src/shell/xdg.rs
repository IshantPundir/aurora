use smithay::{

    input::Seat,
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server:: protocol::{wl_output, wl_seat, wl_surface::WlSurface},
    },
    utils::Serial,
    wayland::shell::xdg::{Configure, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState}
};

use crate::{
    // focus::KeyboardFocusTarget,
    // shell::TouchMoveSurfaceGrab,
    state::{AuroraState, Backend},
};

/* 
Implements the **XDG Shell protocol** for the Wayland compositor. 
The XDG Shell is essential for managing the lifecycle of **application windows**.

**What it does:**
- Handles client requests to **create, resize, and close windows**.
- Manages **window states** like maximized, fullscreen, or tiled.
- Coordinates app windows with compositor elements like popups, tooltips, and sub-surfaces.

**Why it's needed:**
Without `XdgShellHandler`, the compositor would not be able to display standard 
app windows. XDG Shell is the core protocol that allows apps (like browsers, text editors, 
and terminals) to request surfaces and control their appearance on the screen.

This is **crucial for any graphical user interface (GUI) environment**. 
Without it, you'd only be able to display raw surfaces, not "windows" as users expect.
*/
impl<BackendData: Backend> XdgShellHandler for AuroraState<BackendData> {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, _surface: ToplevelSurface) {
    }

    fn toplevel_destroyed(&mut self, _surface: ToplevelSurface) {
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {
    }

    fn reposition_request(&mut self, _surface: PopupSurface, _positioner: PositionerState, _token: u32) {

    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {

    }

    fn resize_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial, _edges: xdg_toplevel::ResizeEdge) {}

    fn ack_configure(&mut self, _surface: WlSurface, _configure: Configure) {

    }

    fn fullscreen_request(&mut self, _surface: ToplevelSurface, mut _wl_output: Option<wl_output::WlOutput>) { }

    fn unfullscreen_request(&mut self, _surface: ToplevelSurface) { }

    fn maximize_request(&mut self, _surface: ToplevelSurface) { }

    fn unmaximize_request(&mut self, _surface: ToplevelSurface) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {

    }
}

impl<BackendData: Backend> AuroraState<BackendData> {
    pub fn move_request_xdg(&mut self, _surface: &ToplevelSurface, _seat: &Seat<Self>, _serial: Serial) {
    }
}