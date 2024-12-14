use smithay::{
    backend::renderer::{
        damage::{Error as OutputDamageTrackerError, OutputDamageTracker, RenderOutputResult},
        element::{
            surface::WaylandSurfaceRenderElement,
            utils::{
                ConstrainAlign, ConstrainScaleBehavior, CropRenderElement, RelocateRenderElement,
                RescaleRenderElement,
            },
            AsRenderElements, RenderElement, Wrap,
        },
        ImportAll, ImportMem, Renderer,
    },
    desktop::space::{
        constrain_space_element, ConstrainBehavior, ConstrainReference, Space, SpaceRenderElements,
    },
    output::Output,
    utils::{Point, Rectangle, Size},
};


use crate::shell::{FullscreenSurface, WindowElement, WindowRenderElement};

smithay::backend::renderer::element::render_elements! {
    pub CustomRenderElements<R> where
        R: ImportAll + ImportMem;
    Surface=WaylandSurfaceRenderElement<R>,
}

pub static CLEAR_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub static CLEAR_COLOR_FULLSCREEN: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

impl<R: Renderer> std::fmt::Debug for CustomRenderElements<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(arg0) => f.debug_tuple("Surface").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}

// Macro to define the `OutputRenderElements` enum, which represents the different types of renderable elements 
// that can be displayed on an output screen. 
//
// This macro generates an enum with variants corresponding to the different types of elements that can be 
// rendered on an output. Each variant represents a specific type of render element. The goal of this design 
// is to have a unified abstraction for rendering elements from different sources (like space elements, 
// windows, custom elements, and previews) into a single renderable unit.
//
// The `where` clause restricts the types used in this enum, ensuring that only render elements compatible 
// with the `ImportAll` and `ImportMem` traits can be used as part of `OutputRenderElements`. 
smithay::backend::renderer::element::render_elements! {
    pub OutputRenderElements<R, E> where R: ImportAll + ImportMem;
    Space=SpaceRenderElements<R, E>,
    Window=Wrap<E>,
    Custom=CustomRenderElements<R>,
    Preview=CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>,
}

impl<R: Renderer + ImportAll + ImportMem, E: RenderElement<R> + std::fmt::Debug> std::fmt::Debug
    for OutputRenderElements<R, E>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Space(arg0) => f.debug_tuple("Space").field(arg0).finish(),
            Self::Window(arg0) => f.debug_tuple("Window").field(arg0).finish(),
            Self::Custom(arg0) => f.debug_tuple("Custom").field(arg0).finish(),
            Self::Preview(arg0) => f.debug_tuple("Preview").field(arg0).finish(),
            Self::_GenericCatcher(arg0) => f.debug_tuple("_GenericCatcher").field(arg0).finish(),
        }
    }
}
/*
This function generates a collection of renderable preview elements for all windows in a given space on a specific output.

The primary purpose of this function is to create a "preview" of all the elements (like windows) that exist on a specific output.
It arranges these elements in a grid layout, ensuring that each window fits within a small preview frame with padding between previews.
This can be useful for features like an "overview mode" where users can see and select windows from a grid of previews.
*/
pub fn space_preview_elements<'a, R, C>(
    renderer: &'a mut R,
    space: &'a Space<WindowElement>,
    output: &'a Output,
) -> impl Iterator<Item = C> + 'a
where
    R: Renderer + ImportAll + ImportMem, // The renderer must support texture imports and memory imports
    R::TextureId: Clone + 'static, // The texture ID must be clonable and have a static lifetime
    C: From<CropRenderElement<RelocateRenderElement<RescaleRenderElement<WindowRenderElement<R>>>>> + 'a, // Complex conversion trait for creating preview elements
{
    // **1. Layout Constraints**
    // The behavior for how each preview is constrained within its bounding box.
    let constrain_behavior = ConstrainBehavior {
        reference: ConstrainReference::BoundingBox, // The preview is constrained relative to its bounding box.
        behavior: ConstrainScaleBehavior::Fit, // Ensure the preview scales to fit inside its container.
        align: ConstrainAlign::CENTER, // Center-align the window in its preview box.
    };

    let preview_padding = 10; // Padding around each preview in the grid.

    // **2. Calculate the total number of elements and space constraints**
    let elements_on_space = space.elements_for_output(output).count(); // Total number of windows/elements in the space.
    let output_scale = output.current_scale().fractional_scale(); // Current fractional scale factor of the output.
    let output_transform = output.current_transform(); // Transformation applied to the output (like rotation, etc.).
    
    let output_size = output
        .current_mode()
        .map(|mode| {
            output_transform
                .transform_size(mode.size) // Transform the output size based on its transformation (e.g., rotation).
                .to_f64()
                .to_logical(output_scale) // Convert the physical pixel size to logical size using the current scale.
        })
        .unwrap_or_default(); // Default to (0,0) size if the mode is not available.

    // **3. Calculate the number of rows and columns in the preview grid**
    let max_elements_per_row = 4; // Maximum number of previews per row.
    let elements_per_row = usize::min(elements_on_space, max_elements_per_row); // Use either the max or the total number of elements, whichever is smaller.
    let rows = f64::ceil(elements_on_space as f64 / elements_per_row as f64); // Total number of rows needed to display all elements.

    // **4. Calculate the size for each preview box**
    let preview_size = Size::from((
        f64::round(output_size.w / elements_per_row as f64) as i32 - preview_padding * 2, // Width of each preview box.
        f64::round(output_size.h / rows) as i32 - preview_padding * 2, // Height of each preview box.
    ));

    // **5. Arrange and render each element as a preview**
    space
        .elements_for_output(output) // Get all elements on the given output.
        .enumerate() // Enumerate to get index (used for row/column calculation) and element.
        .flat_map(move |(element_index, window)| {
            // **6. Calculate which row and column this element should be in**
            let column = element_index % elements_per_row; // Column index of the preview (based on modulo of total per row).
            let row = element_index / elements_per_row; // Row index (based on integer division).
            
            // **7. Calculate the position of this preview in the grid**
            let preview_location = Point::from((
                preview_padding + (preview_padding + preview_size.w) * column as i32, // X position
                preview_padding + (preview_padding + preview_size.h) * row as i32, // Y position
            ));
            
            // **8. Constrain the element to fit inside the preview box**
            let constrain = Rectangle::from_loc_and_size(preview_location, preview_size); // The bounding box for this preview.
            
            // **9. Use the constrain logic to render the window as a preview**
            constrain_space_element(
                renderer, // The renderer responsible for drawing the element.
                window, // The current window to be constrained.
                preview_location, // Position of the preview in the grid.
                1.0, // Scale factor for this preview (1.0 = no scaling).
                output_scale, // Scale factor of the output.
                constrain, // The constraint bounds (where the element must fit inside).
                constrain_behavior, // Behavior for how the element is constrained.
            )
        })
}
/*
Generates the render elements for an output, including fullscreen windows, previews, and space elements.

# Arguments
- `output`: The output for which the elements are generated.
- `space`: A reference to the space containing the window elements.
- `custom_elements`: A collection of custom render elements to be included in the output.
- `renderer`: The renderer used to create the render elements.
- `show_window_preview`: A flag indicating whether to generate window previews.

# Returns
- A tuple containing:
  - A vector of render elements that will be drawn on the output.
  - A color used to clear the output (background color).
# Example Usage
This function is typically called within the rendering pipeline to prepare the elements that will be drawn on an output.
*/
pub fn output_elements<R>(
    output: &Output,
    space: &Space<WindowElement>,
    custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
    renderer: &mut R,
    show_window_preview: bool,
) -> (Vec<OutputRenderElements<R, WindowRenderElement<R>>>, [f32; 4])
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    if let Some(window) = output
        .user_data()
        .get::<FullscreenSurface>()
        .and_then(|f| f.get())
    {
        // Handle fullscreen window rendering
        let scale = output.current_scale().fractional_scale().into();
        let window_render_elements: Vec<WindowRenderElement<R>> =
            AsRenderElements::<R>::render_elements(&window, renderer, (0, 0).into(), scale, 1.0);

        let elements = custom_elements
            .into_iter()
            .map(OutputRenderElements::from)
            .chain(
                window_render_elements
                    .into_iter()
                    .map(|e| OutputRenderElements::Window(Wrap::from(e))),
            )
            .collect::<Vec<_>>();
        
        (elements, CLEAR_COLOR_FULLSCREEN)
    } else {
        // Handle standard rendering with space and optional window previews
        let mut output_render_elements = custom_elements
            .into_iter()
            .map(OutputRenderElements::from)
            .collect::<Vec<_>>();

        if show_window_preview && space.elements_for_output(output).count() > 0 {
            output_render_elements.extend(space_preview_elements(renderer, space, output));
        }

        let space_elements = smithay::desktop::space::space_render_elements::<_, WindowElement, _>(
            renderer,
            [space],
            output,
            1.0,
        )
        .expect("output without mode?");
        
        output_render_elements.extend(space_elements.into_iter().map(OutputRenderElements::Space));

        (output_render_elements, CLEAR_COLOR)
    }
}

/*
Renders the elements for an output using the damage tracker to optimize rendering.

# Arguments
- `output`: The output for which the rendering is performed.
- `space`: A reference to the space containing the window elements.
- `custom_elements`: A collection of custom render elements to be included in the output.
- `renderer`: The renderer used to render the elements.
- `damage_tracker`: Tracks damage to the output, allowing for optimized partial rendering.
- `age`: The "age" of the damage, used to determine which areas to re-render.
- `show_window_preview`: A flag indicating whether to render window previews.

# Returns
- A `RenderOutputResult`, containing information about the rendering result.
*/
pub fn render_output<'a, 'd, R>(
    output: &'a Output,
    space: &'a Space<WindowElement>,
    custom_elements: impl IntoIterator<Item = CustomRenderElements<R>>,
    renderer: &'a mut R,
    damage_tracker: &'d mut OutputDamageTracker,
    age: usize,
    show_window_preview: bool,
) -> Result<RenderOutputResult<'d>, OutputDamageTrackerError<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Clone + 'static,
{
    // Generate elements to be rendered and background clear color
    // Calls `output_elements` to gather all the elements that should be rendered on the output.
    let (elements, clear_color) = output_elements(output, space, custom_elements, renderer, show_window_preview);
    
    // Render the output using the damage tracker, optimizing for only changed areas
    damage_tracker.render_output(renderer, age, &elements, clear_color)
} 

