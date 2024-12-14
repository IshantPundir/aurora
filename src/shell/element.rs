use std::{borrow::Cow, time::Duration};

use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement, AsRenderElements},
        ImportAll, ImportMem, Renderer, Texture,
    },
    desktop::{
        space::SpaceElement, utils::OutputPresentationFeedback, Window, WindowSurface, WindowSurfaceType,
    },
    output::Output,
    reexports::{
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    render_elements,
    utils::{user_data::UserDataMap, IsAlive, Logical, Physical, Point, Scale},
    wayland::{compositor::SurfaceData as WlSurfaceData, dmabuf::DmabufFeedback, seat::WaylandFocus},
};
use crate::focus::PointerFocusTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowElement(pub Window);

impl IsAlive for WindowElement {
    #[inline]
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl SpaceElement for WindowElement {
    fn geometry(&self) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        let geo = SpaceElement::geometry(&self.0);
        geo
    }

    fn bbox(&self) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        let bbox = SpaceElement::bbox(&self.0);
        bbox
    }

    fn is_in_input_region(&self, point: &smithay::utils::Point<f64, smithay::utils::Logical>) -> bool {
        SpaceElement::is_in_input_region(&self.0, point)
    }

    fn z_index(&self) -> u8 {
        SpaceElement::z_index(&self.0)
    }

    fn set_activate(&self, activated: bool) {
        SpaceElement::set_activate(&self.0, activated);
    }

    fn output_enter(&self, output: &smithay::output::Output, overlap: smithay::utils::Rectangle<i32, smithay::utils::Logical>) {
        SpaceElement::output_enter(&self.0, output, overlap);
    }

    fn output_leave(&self, output: &smithay::output::Output) {
        SpaceElement::output_leave(&self.0, output);
    }

    fn refresh(&self) {
        SpaceElement::refresh(&self.0);
    }
}

impl WindowElement {
    /*
    **Finds the surface under a given point relative to the window.**
    
    **Parameters:**
    - `location`: A point (x, y) in logical coordinates where we are checking for a surface.
    - `window_type`: The type of surface to look for (e.g., XDG surface, subsurface, etc.).
    
    **Returns:**
    - `Option<(PointerFocusTarget, Point<i32, Logical>)>`: 
      - `PointerFocusTarget`: Identifies which surface is being "focused" (like which part of the window the pointer is over).
      - `Point<i32, Logical>`: The exact location of the pointer relative to the surface.

    This method is used for pointer hit-testing to determine which part of the window is under 
    the cursor. If the cursor is over the window, it returns the surface and location; otherwise, `None`.
    */
    pub fn surface_under(
        &self,
        location: Point<f64, Logical>,
        window_type: WindowSurfaceType,
    ) -> Option<(PointerFocusTarget, Point<i32, Logical>)> {
        // An offset, usually used for handling relative positioning (like window decorations).
        let offset = Point::default();

        // Adjust the location by subtracting the offset to account for decorations or margins.
        let surface_under = self.0.surface_under(location - offset.to_f64(), window_type);

        // Determine which surface is underneath, and if it exists, wrap it in a `PointerFocusTarget`.
        let (under, loc) = match self.0.underlying_surface() {
            WindowSurface::Wayland(_) => {
                surface_under.map(|(surface, loc)| (PointerFocusTarget::WlSurface(surface), loc))
            }
        }?;

        Some((under, loc + offset))
    }
    /*
    **Iterates over all surfaces in the window and processes them.**
    
    **Parameters:**
    - `processor`: A closure (function) that takes a reference to a `WlSurface` and its associated data (`WlSurfaceData`).
    
    **Use Case:**
    This method allows traversal over all the surfaces in a window (main surface and sub-surfaces)
    and executes a provided function on each. This is useful for tasks like rendering or updating 
    surface properties.
    */
    pub fn with_surfaces<F>(&self, processor: F)
    where
        F: FnMut(&WlSurface, &WlSurfaceData),
    {
        self.0.with_surfaces(processor);
    }
    /*
    **Sends a frame event to the surface, notifying it that a new frame is ready.**
    
    **Parameters:**
    - `output`: The output (like a monitor or display) where the frame is being displayed.
    - `time`: The time associated with the frame (used for animations or frame timing).
    - `throttle`: An optional delay to slow down frame dispatching.
    - `primary_scan_out_output`: A closure that can choose the primary scan-out output.
    
    **Use Case:**
    This is used to notify Wayland clients that a new frame is ready to be drawn. 
    The time and throttle are used to control frame pacing, and the output is needed 
    to indicate which display is targeted.
    */
    pub fn send_frame<T, F>(
        &self,
        output: &Output,
        time: T,
        throttle: Option<Duration>,
        primary_scan_out_output: F,
    ) where
        T: Into<Duration>,
        F: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
    {
        self.0.send_frame(output, time, throttle, primary_scan_out_output)
    }
    /*
    **Sends DMA-BUF feedback to the surface.**
    
    **Parameters:**
    - `output`: The output (like a monitor or display) the DMA-BUF feedback is relevant to.
    - `primary_scan_out_output`: Closure to select the primary output.
    - `select_dmabuf_feedback`: Closure to select the DMA-BUF feedback for the surface.
    
    **Use Case:**
    DMA-BUF (Direct Memory Access Buffer) is used for zero-copy rendering. This method 
    sends feedback to Wayland clients about how their buffers are being handled by the 
    compositor, which can optimize client-side rendering.
    */
    pub fn send_dmabuf_feedback<'a, P, F>(
        &self,
        output: &Output,
        primary_scan_out_output: P,
        select_dmabuf_feedback: F,
    ) where
        P: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F: Fn(&WlSurface, &WlSurfaceData) -> &'a DmabufFeedback + Copy,
    {
        self.0
            .send_dmabuf_feedback(output, primary_scan_out_output, select_dmabuf_feedback)
    }

    /*
    **Takes presentation feedback for the surface.**
    
    **Parameters:**
    - `output_feedback`: Feedback about how the frame was presented (latency, frame timing, etc.).
    - `primary_scan_out_output`: Closure that chooses which output was used for scanning.
    - `presentation_feedback_flags`: Closure that defines presentation feedback flags.
    
    **Use Case:**
    This method collects presentation feedback from Wayland clients, letting them know 
    about frame timing, presentation latency, and other details. It is useful for optimizing 
    frame presentation or for animations requiring smooth frame timing.
    */
    pub fn take_presentation_feedback<F1, F2>(
        &self,
        output_feedback: &mut OutputPresentationFeedback,
        primary_scan_out_output: F1,
        presentation_feedback_flags: F2,
    ) where
        F1: FnMut(&WlSurface, &WlSurfaceData) -> Option<Output> + Copy,
        F2: FnMut(&WlSurface, &WlSurfaceData) -> wp_presentation_feedback::Kind + Copy,
    {
        self.0.take_presentation_feedback(
            output_feedback,
            primary_scan_out_output,
            presentation_feedback_flags,
        )
    }
    /*
    **Checks if the window is a Wayland surface.**
    
    **Returns:**
    - `true` if the window is using a Wayland surface.
    - `false` otherwise.
    
    **Use Case:**
    This is a simple type check to see if the window is backed by a Wayland surface. 
    It's often used when different handling logic is needed for Wayland-specific surfaces.
    */
    #[inline]
    pub fn is_wayland(&self) -> bool {
        self.0.is_wayland()
    }
    /*
    **Gets the associated Wayland surface, if available.**
    
    **Returns:**
    - `Option<Cow<WlSurface>>`: A reference to the `WlSurface`, if it exists.
    
    **Use Case:**
    This method provides access to the underlying Wayland surface, which can be 
    useful for sending Wayland-specific commands or accessing Wayland-specific properties.
    */
    #[inline]
    pub fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        self.0.wl_surface()
    }
    /*
    **Gets the user data map associated with the window.**
    
    **Returns:**
    - `&UserDataMap`: A reference to the user data map.
    
    **Use Case:**
    The user data map allows developers to store custom data (like metadata) 
    associated with the window. It acts as a key-value store to attach user-defined 
    properties to the window.
    */
    #[inline]
    pub fn user_data(&self) -> &UserDataMap {
        self.0.user_data()
    }
}



impl<R: Renderer> std::fmt::Debug for WindowRenderElement<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Decoration(arg0) => f.debug_tuple("Decoration").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

impl<R> AsRenderElements<R> for WindowElement
where
    R: Renderer + ImportAll + ImportMem,
    <R as Renderer>::TextureId: Clone + Texture + 'static,
{
    type RenderElement = WindowRenderElement<R>;

    fn render_elements<C: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Vec<C> {        
        AsRenderElements::render_elements(&self.0, renderer, location, scale, alpha)
            .into_iter()
            .map(C::from)
            .collect()
    
    }
}

render_elements!(
    pub WindowRenderElement<R> where R: ImportAll + ImportMem;
    Window=WaylandSurfaceRenderElement<R>,
    Decoration=SolidColorRenderElement,
);