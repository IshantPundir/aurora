use smithay::{desktop::Space, reexports::wayland_protocols::xdg::shell::server::xdg_toplevel, utils::IsAlive};

use crate::shell::WindowElement;

#[derive(Debug)]
pub struct WindowManager {
    windows: Vec<WindowElement>
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: Vec::new()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.windows.len() == 0
    }

    pub fn insert_window(&mut self, window: WindowElement) {
        self.windows.push(window.clone());
    }

    pub fn get_active_window(&self) -> Option<&WindowElement> {
        self.windows.last()
    }

    fn remove_dead_window(&mut self) {
        self.windows.retain(|w| w.alive());
    }

    pub fn refresh_geometry(&mut self, space: &mut Space<WindowElement>) {
        space.refresh();

        // Remove dead windows/closed apps.
        self.remove_dead_window();

        // Get the first output available & its geometry;
        let output = space.outputs().next().cloned().unwrap();
        let output_geometry = space.output_geometry(&output).unwrap();
        
        // Only display the window at the last index of windows::Vec
        for (i, window) in self.windows.iter().rev().enumerate() {
            if i == 0 {
                
                // Render this app to output;
                if let Some(toplevel) = window.0.toplevel() {
                    toplevel.with_pending_state(|state| {
                        state.size = Some((output_geometry.size.w, output_geometry.size.h).into());
                        state.states.set(xdg_toplevel::State::Fullscreen);
                    });

                    toplevel.send_pending_configure();

                    if toplevel.is_initial_configure_sent() {
                        toplevel.send_configure();
                    };
                }

                space.map_element(window.clone(), (0, 0), true);
            } else {
                space.unmap_elem(window);
            }
         }
    }
}