use std::slice::Windows;

use smithay::{desktop::Space, utils::IsAlive};

use crate::shell::WindowElement;

#[derive(Debug)]
pub struct AppManger {
    apps: Vec<WindowElement>
}

impl AppManger {
    pub fn new() -> Self {
        Self {
            apps: Vec::new()
        }
    }

    pub fn insert_window(&mut self, window: WindowElement) {
        // Insert apps to apps vector
        self.apps.push(window.clone());
        // TODO: Set this app as the active app?
    }

    pub fn remove_dead_window(&mut self) {
        self.apps.retain(|w| w.alive());
    }

    pub fn refresh_geometry(&mut self, space: &mut Space<WindowElement>) {
        space.refresh();

        // Remove dead elements; ie: closed apps;
        self.remove_dead_window();

        // Get the first output available;
        let output = space.outputs().next().cloned().unwrap();

        // Find the size of output;
        let output_geometry = space.output_geometry(&output).unwrap();
        let output_width = output_geometry.size.w;
        let output_height = output_geometry.size.h;

        let apps_count = self.apps.len() as i32;
        
        for (i, window) in self.apps.iter().enumerate() {
            let (mut x, mut y) = (0, 0);
            let (mut width, mut height) = (output_width, output_height);
        
            if apps_count > 1 {
                width /= 2;
            }

            if i > 0 {
                height /= apps_count - 1;
                x += width;
                y += height * (i as i32 - 1);
            }

            if let Some(toplevel) = window.0.toplevel() {
                toplevel.with_pending_state(|state| {
                    state.size = Some((width, height).into());
                });

                toplevel.send_pending_configure();
            }

            space.map_element(window.clone(), (x, y), false);
        }
    }
}