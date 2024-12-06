use smithay::{desktop::Space, utils::IsAlive};

use crate::shell::WindowElement;

#[derive(Debug)]
struct Workspace {
    windows: Vec<WindowElement>,
    active_window: Option<usize>
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            active_window: None
        }
    }    
}

#[derive(Debug)]
pub struct Workspaces {
    workspaces: Vec<Workspace>,
    active_workspace: usize
}

impl Workspaces {
    pub fn new() -> Self {
        Self {
            workspaces: (0..8).map(|_| Workspace::new()).collect(),
            active_workspace: 0
        }
    }

    pub fn active(&self) -> usize {
        self.active_workspace
    }

    pub fn set_active_window(&mut self, workspace: usize, window: WindowElement) {
        let workspace = &mut self.workspaces[workspace];
        workspace.active_window = workspace.windows.iter().position(|w| w == &window);
    }

    pub fn insert_window(&mut self, workspace: usize, window: WindowElement) {
        self.workspaces[workspace].windows.push(window.clone());

        if self.workspaces[workspace].windows.len() == 1 {
            self.set_active_window(workspace, window);
        }
    }

    pub fn remove_dead_window(&mut self) {
        self.workspaces.iter_mut().for_each(|x| x.windows.retain(|w| w.alive()));
    }

    pub fn refresh_geometry(&mut self, space: &mut Space<WindowElement>) {
        space.refresh();

        // Remove dead elements from workspaces;
        self.remove_dead_window();

        // TODO: Hide the previous active workspace.

        // Get the first output available;
        let output = space.outputs().next().cloned().unwrap();

        // Find the size of output;
        let output_geometry = space.output_geometry(&output).unwrap();
        let output_width = output_geometry.size.w;
        let output_height = output_geometry.size.h;

        let gap = 6;

        let windows = &mut self.workspaces[self.active_workspace].windows;

        let elements_count = windows.len() as i32;

        for (i, window) in windows.iter().enumerate() {
            // Move the window to start at the gap size creating a gap around window;
            let (mut x, mut y) = (gap, gap);
            let (mut width, mut height) = (output_width - gap * 2, output_height - gap * 2);

            // If there is more than one window, subtract an additional gap from the width and
            // device the width in two giving room for another window;
            if elements_count > 1 {
                width -= gap;
                width /= 2;
            }

            if i > 0 {
                height /= elements_count - 1;

                x += width + gap;
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