use smithay::{desktop::Space, utils::IsAlive};

use crate::shell::WindowElement;


#[derive(Debug)]
enum Mode {
    INTERACTIVE,
    PREVIEW
}

#[derive(Debug)]
pub struct AppManger {
    apps: Vec<WindowElement>,
    mode: Mode
}

impl AppManger {
    pub fn new() -> Self {
        Self {
            apps: Vec::new(),
            mode: Mode::PREVIEW
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


        match self.mode {
            Mode::INTERACTIVE => {
                // In INTERACTIVE full screen;
                // Only display the app at the last index of apps::VEC;
                // Unmap all of the other windows;
                for (i, window) in self.apps.iter().rev().enumerate() {
                    if i == 0 {
                        let (x, y) = (0, 0);
        
                        if let Some(toplevel) = window.0.toplevel() {
                            toplevel.with_pending_state(|state| {
                                state.size = Some((output_width, output_height).into());
                            });
        
                            toplevel.send_pending_configure();
                        }
        
                        space.map_element(window.clone(), (x, y), true);
                    }
                    else {
                        space.unmap_elem(window);
                    }
                }
            }

            Mode::PREVIEW => {
                // In select mode show all of the active windows side by side;
                // This allows user to manage apps, eg: selecting active app, close an app, etc.
                
                let padding = 200;
                for (i, window) in self.apps.iter().rev().enumerate() {
                    let window_height = output_height - padding * 2;
                    let window_width = output_width - padding * 2;
                    let (mut x, y) = (padding, padding);

                    if i > 0 {
                        x = window_width * (i as i32) + padding;
                    //     y += 50;
                    //     window_height -= 100;
                    }

                    if let Some(toplevel) = window.0.toplevel() {
                        toplevel.with_pending_state(|state| {
                            state.size = Some((window_width, window_height).into());
                        });
    
                        toplevel.send_pending_configure();
                    }
    
                    space.map_element(window.clone(), (x, y), true);
                }
            }
        }
    }

    //     let apps_count = self.apps.len() as i32;

        
        
    //     for (i, window) in self.apps.iter().enumerate() {
    //         let (mut x, mut y) = (0, 0);
    //         let (mut width, mut height) = (output_width, output_height);
        
    //         if apps_count > 1 {
    //             width /= 2;
    //         }

    //         if i > 0 {
    //             height /= apps_count - 1;
    //             x += width;
    //             y += height * (i as i32 - 1);
    //         }

    //         if let Some(toplevel) = window.0.toplevel() {
    //             toplevel.with_pending_state(|state| {
    //                 state.size = Some((width, height).into());
    //             });

    //             toplevel.send_pending_configure();
    //         }

    //         space.map_element(window.clone(), (x, y), true);
    //     }
    // }
}