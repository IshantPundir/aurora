use std::cell::RefCell;

use smithay::wayland::drm_syncobj::DrmSyncobjCachedState;

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{
        layer_map_for_output, space::SpaceElement, LayerSurface, PopupKind, PopupManager, Space,
        WindowSurfaceType,
    },
    output::Output,
    reexports::{
        calloop::Interest,
        wayland_server::{
            protocol::{wl_output, wl_surface::WlSurface},
            Client, Resource,
        },
    },
    utils::{IsAlive, Logical, Point, Rectangle, Size},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states,
            with_surface_tree_upward, BufferAssignment, CompositorClientState, CompositorHandler,
            CompositorState, SurfaceAttributes, TraversalAction,
        },
        dmabuf::get_dmabuf,
        shell::{
            wlr_layer::{
                Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
                WlrLayerShellState,
            },
            xdg::XdgToplevelSurfaceData,
        },
    },
};


use crate::window_manager::WindowManager;
use crate::ClientState;
use crate::{state::Backend, AuroraState};

pub use self::element::*;

mod element;
mod xdg;

/* 
Structure to store data about a surface, such as geometry.
*/
#[derive(Default)]
pub struct SurfaceData {
    pub geometry: Option<Rectangle<i32, Logical>>,
}

impl<BackendData: Backend> AuroraState<BackendData> {
    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<WindowElement> {
        self.space
            .elements()
            .find(|window| window.wl_surface().map(|s| &*s == surface).unwrap_or(false))
            .cloned()
    }
}

#[derive(Default)]
pub struct FullscreenSurface(RefCell<Option<WindowElement>>);

impl FullscreenSurface {
    pub fn set(&self, window: WindowElement) {
        *self.0.borrow_mut() = Some(window);
    }

    pub fn get(&self) -> Option<WindowElement> {
        let mut window = self.0.borrow_mut();
        if window.as_ref().map(|w| !w.alive()).unwrap_or(false) {
            *window = None;
        }
        window.clone()
    }

    pub fn clear(&self) -> Option<WindowElement> {
        self.0.borrow_mut().take()
    }
}

/*
Implementation of the `CompositorHandler` for the `AuroraState` struct.
The CompositorHandler defines how the compositor state, client compositor state,
and surface-related events like new surfaces and commits are handled.
*/
impl<BackendData: Backend> CompositorHandler for AuroraState<BackendData> {
    /* 
    Returns a mutable reference to the CompositorState.
    This allows for direct manipulation of the compositor's state.
    */
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    /* 
    Retrieves the compositor state associated with a specific client.
    If the client data is missing, the program will panic.
    */
    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(state) = client.get_data::<ClientState>() {
            return &state.compositor_state;
        }
        panic!("Unknown client data type")
    }

    /*
    Handles the creation of a new surface.
    Attaches a pre-commit hook to the surface to handle buffer acquisition and synchronization points.
    */
    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, move |state, _dh, surface| {
            let mut acquire_point = None;

            // Check if the surface has a buffer attached and attempt to acquire a dmabuf for it.
            let maybe_dmabuf: Option<smithay::backend::allocator::dmabuf::Dmabuf> = with_states(surface, |surface_data| {
                acquire_point.clone_from(
                    &surface_data
                        .cached_state
                        .get::<DrmSyncobjCachedState>()
                        .pending()
                        .acquire_point,
                );
                surface_data
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .pending()
                    .buffer
                    .as_ref()
                    .and_then(|assignment| match assignment {
                        BufferAssignment::NewBuffer(buffer) => get_dmabuf(buffer).cloned().ok(),
                        _ => None,
                    })
            });

            // Handle synchronization logic for the dmabuf, if it exists.
            if let Some(dmabuf) = maybe_dmabuf {
                if let Some(acquire_point) = acquire_point {
                    if let Ok((blocker, source)) = acquire_point.generate_blocker() {
                        let client = surface.client().unwrap();
                        let res = state.handle.insert_source(source, move |_, _, data| {
                            let dh = data.display_handle.clone();
                            data.client_compositor_state(&client).blocker_cleared(data, &dh);
                            Ok(())
                        });
                        if res.is_ok() {
                            add_blocker(surface, blocker);
                            return;
                        }
                    }
                }

                if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
                    if let Some(client) = surface.client() {
                        let res = state.handle.insert_source(source, move |_, _, data| {
                            let dh = data.display_handle.clone();
                            data.client_compositor_state(&client).blocker_cleared(data, &dh);
                            Ok(())
                        });
                        if res.is_ok() {
                            add_blocker(surface, blocker);
                        }
                    }
                }
            }
        });
    }

    /* 
    Handles the commit event for a surface.
    This updates the internal state, moves surfaces if necessary, and ensures the initial configure is sent.
    */
    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.backend_data.early_import(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_for_surface(&root) {
                window.0.on_commit();

                if &root == surface {
                    let buffer_offset = with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .buffer_delta
                            .take()
                    });

                    if let Some(buffer_offset) = buffer_offset {
                        let current_loc = self.space.element_location(&window).unwrap();
                        self.space.map_element(window, current_loc + buffer_offset, false);
                    }
                }
            }
        }
        self.popups.commit(surface);
        ensure_initial_configure(surface, &self.space, &mut self.popups)
    }
}

impl<BackendData: Backend> WlrLayerShellHandler for AuroraState<BackendData> {
    /* 
       This function returns a mutable reference to the WlrLayerShellState. 
       The state manages the current status of the WLR layer shell and tracks 
       all the layer surfaces associated with it. 
    */
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    /* 
       This function is called whenever a new WLR layer surface is created. 
       
       Parameters:
       - surface: The newly created WLR layer surface.
       - wl_output: The Wayland output (monitor) on which this layer surface will be displayed.
       - _layer: The layer level (e.g., background, bottom, top, or overlay) for the surface.
       - namespace: A string identifier for the surface, used to group or categorize the layer surface.
       
       If no specific Wayland output is provided, it defaults to the first output available in the space.
       The layer surface is then mapped (positioned and displayed) on the specified output.
    */
    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        wl_output: Option<wl_output::WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // If an output is provided, use it. Otherwise, select the first output in the space.
        let output = wl_output
            .as_ref()
            .and_then(Output::from_resource) // Tries to get an Output object from the Wayland resource.
            .unwrap_or_else(|| self.space.outputs().next().unwrap().clone()); // Defaults to the first available output.

        // Get the layer map for the selected output.
        let mut map = layer_map_for_output(&output);

        // Map (position and display) the layer surface on the output.
        // If mapping fails, it returns an error (which is currently not handled).
        map.map_layer(&LayerSurface::new(surface, namespace)).unwrap();
    }

    /* 
       This function is called when a WLR layer surface is destroyed.
       
       Parameters:
       - surface: The WLR layer surface that is being destroyed.
       
       The function finds the output (monitor) where the surface was displayed, 
       retrieves its layer map, and removes (unmaps) the layer surface from the map.
    */
    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        if let Some((mut map, layer)) = self.space.outputs().find_map(|o| {
            // Get the layer map for the current output.
            let map = layer_map_for_output(o);
            // Find the specific layer associated with the surface in this output's layer list.
            let layer = map
                .layers()
                .find(|&layer| layer.layer_surface() == &surface) // Look for the matching surface.
                .cloned(); // Clone the layer reference so it can be returned.
            layer.map(|layer| (map, layer)) // Return both the map and the layer.
        }) {
            // If a matching layer is found, unmap it from the layer map.
            map.unmap_layer(&layer);
        }
    }
}

impl <BackendData: Backend> BufferHandler for AuroraState<BackendData> {
    fn buffer_destroyed(&mut self, _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer) { }
}
/* 
Ensures the initial configure event is sent to the surface.
This event is used to notify the client about the initial state of the surface.
*/
fn ensure_initial_configure(surface: &WlSurface, space: &Space<WindowElement>, popups: &mut PopupManager) {
    with_surface_tree_upward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |_, states, _| {
            states
                .data_map
                .insert_if_missing(|| RefCell::new(SurfaceData::default()));
        },
        |_, _, _| true,
    );

    if let Some(window) = space
        .elements()
        .find(|window| window.wl_surface().map(|s| &*s == surface).unwrap_or(false))
        .cloned()
    {
        if let Some(toplevel) = window.0.toplevel() {
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            });
            if !initial_configure_sent {
                toplevel.send_configure();
            }
        }
    }

    if let Some(popup) = popups.find_popup(surface) {
        let popup = match popup {
            PopupKind::Xdg(ref popup) => popup,
            PopupKind::InputMethod(ref _input_popup) => {
                return;
            }
        };

        if !popup.is_initial_configure_sent() {
            popup.send_configure().expect("initial configure failed");
        }
        return;
    };

    if let Some(output) = space.outputs().find(|o| {
        let map = layer_map_for_output(o);
        map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .is_some()
    }) {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        let mut map = layer_map_for_output(output);
        map.arrange();

        if !initial_configure_sent {
            let layer = map
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .unwrap();
            layer.layer_surface().send_configure();
        }
    };
}

fn place_new_window(
    space: &mut Space<WindowElement>,
    pointer_location: Point<f64, Logical>,
    window: &WindowElement,
    activate: bool,
) {
    // place the window at a random location on same output as pointer
    // or if there is not output in a [0;800]x[0;800] square
    use rand::distributions::{Distribution, Uniform};

    let output = space
        .output_under(pointer_location)
        .next()
        .or_else(|| space.outputs().next())
        .cloned();
    let output_geometry = output
        .and_then(|o| {
            let geo = space.output_geometry(&o)?;
            let map = layer_map_for_output(&o);
            let zone = map.non_exclusive_zone();
            Some(Rectangle::from_loc_and_size(geo.loc + zone.loc, zone.size))
        })
        .unwrap_or_else(|| Rectangle::from_loc_and_size((0, 0), (800, 800)));

    // set the initial toplevel bounds
    #[allow(irrefutable_let_patterns)]
    if let Some(toplevel) = window.0.toplevel() {
        toplevel.with_pending_state(|state| {
            state.bounds = Some(output_geometry.size);
        });
    }

    let max_x = output_geometry.loc.x + (((output_geometry.size.w as f32) / 3.0) * 2.0) as i32;
    let max_y = output_geometry.loc.y + (((output_geometry.size.h as f32) / 3.0) * 2.0) as i32;
    let x_range = Uniform::new(output_geometry.loc.x, max_x);
    let y_range = Uniform::new(output_geometry.loc.y, max_y);
    let mut rng = rand::thread_rng();
    let x = x_range.sample(&mut rng);
    let y = y_range.sample(&mut rng);

    space.map_element(window.clone(), (x, y), activate);
}

pub fn fixup_positions(space: &mut Space<WindowElement>, window_manager: &mut WindowManager, pointer_location: Point<f64, Logical>) {
    // fixup outputs
    let mut offset = Point::<i32, Logical>::from((0, 0));
    for output in space.outputs().cloned().collect::<Vec<_>>().into_iter() {
        let size = space
            .output_geometry(&output)
            .map(|geo| geo.size)
            .unwrap_or_else(|| Size::from((0, 0)));
        space.map_output(&output, offset);
        layer_map_for_output(&output).arrange();
        offset.x += size.w;
    }

    // fixup windows
    let mut orphaned_windows = Vec::new();
    let outputs = space
        .outputs()
        .flat_map(|o| {
            let geo = space.output_geometry(o)?;
            let map = layer_map_for_output(o);
            let zone = map.non_exclusive_zone();
            Some(Rectangle::from_loc_and_size(geo.loc + zone.loc, zone.size))
        })
        .collect::<Vec<_>>();
    for window in space.elements() {
        let window_location = match space.element_location(window) {
            Some(loc) => loc,
            None => continue,
        };
        let geo_loc = window.bbox().loc + window_location;

        if !outputs.iter().any(|o_geo| o_geo.contains(geo_loc)) {
            orphaned_windows.push(window.clone());
        }
    }
    for window in orphaned_windows.into_iter() {
        place_new_window(space, pointer_location, &window, false);
    }

    // Fixup apps???
    window_manager.refresh_geometry(space);
    
}
