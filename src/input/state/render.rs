use crate::draw::{
    Color, Shape, render_freehand_borrowed, render_marker_stroke_borrowed, render_shape,
};
use crate::input::tool::Tool;
use crate::util;

use super::{DrawingState, InputState};

impl InputState {
    /// Returns the shape currently being drawn for live preview.
    ///
    /// # Arguments
    /// * `current_x` - Current mouse X coordinate
    /// * `current_y` - Current mouse Y coordinate
    ///
    /// # Returns
    /// - `Some(Shape)` if actively drawing (for preview rendering)
    /// - `None` if idle or in text input mode
    ///
    /// # Note
    /// For Pen tool (freehand), this clones the points vector. For better performance
    /// with long strokes, consider using `render_provisional_shape` directly with a
    /// borrow instead of calling this method and rendering separately.
    ///
    /// This allows the backend to render a preview of the shape being drawn
    /// before the mouse button is released.
    pub fn get_provisional_shape(&self, current_x: i32, current_y: i32) -> Option<Shape> {
        if let DrawingState::Drawing {
            tool,
            start_x,
            start_y,
            points,
        } = &self.state
        {
            match tool {
                Tool::Pen => Some(Shape::Freehand {
                    points: points.clone(), // TODO: Consider using Cow or separate borrow API
                    color: self.current_color,
                    thick: self.current_thickness,
                    per_point_colors: None,
                }),
                Tool::Line => {
                    let (start_color, end_color) = if self.rainbow_mode_enabled {
                        let dx = (current_x - *start_x) as f64;
                        let dy = (current_y - *start_y) as f64;
                        let distance = (dx * dx + dy * dy).sqrt();
                        let start_hue = self.get_rainbow_hue();
                        let end_hue = start_hue + distance * self.rainbow_hue_step_per_pixel;
                        (Some(self.rainbow_color_from_hue(start_hue)),
                         Some(self.rainbow_color_from_hue(end_hue)))
                    } else {
                        (None, None)
                    };
                    Some(Shape::Line {
                        x1: *start_x,
                        y1: *start_y,
                        x2: current_x,
                        y2: current_y,
                        color: self.current_color,
                        thick: self.current_thickness,
                        start_color,
                        end_color,
                    })
                }
                Tool::Rect => {
                    // Normalize rectangle to handle dragging in any direction
                    let (x, w) = if current_x >= *start_x {
                        (*start_x, current_x - start_x)
                    } else {
                        (current_x, start_x - current_x)
                    };
                    let (y, h) = if current_y >= *start_y {
                        (*start_y, current_y - start_y)
                    } else {
                        (current_y, start_y - current_y)
                    };
                    let (start_color, end_color) = if self.rainbow_mode_enabled {
                        let diagonal = ((w * w + h * h) as f64).sqrt();
                        let start_hue = self.get_rainbow_hue();
                        let end_hue = start_hue + diagonal * self.rainbow_hue_step_per_pixel;
                        (Some(self.rainbow_color_from_hue(start_hue)),
                         Some(self.rainbow_color_from_hue(end_hue)))
                    } else {
                        (None, None)
                    };
                    Some(Shape::Rect {
                        x,
                        y,
                        w,
                        h,
                        fill: self.fill_enabled,
                        color: self.current_color,
                        thick: self.current_thickness,
                        start_color,
                        end_color,
                    })
                }
                Tool::Ellipse => {
                    let (cx, cy, rx, ry) =
                        util::ellipse_bounds(*start_x, *start_y, current_x, current_y);
                    let (start_color, end_color) = if self.rainbow_mode_enabled {
                        let diameter = (rx * 2) as f64;
                        let start_hue = self.get_rainbow_hue();
                        let end_hue = start_hue + diameter * self.rainbow_hue_step_per_pixel;
                        (Some(self.rainbow_color_from_hue(start_hue)),
                         Some(self.rainbow_color_from_hue(end_hue)))
                    } else {
                        (None, None)
                    };
                    Some(Shape::Ellipse {
                        cx,
                        cy,
                        rx,
                        ry,
                        fill: self.fill_enabled,
                        color: self.current_color,
                        thick: self.current_thickness,
                        start_color,
                        end_color,
                    })
                }
                Tool::Arrow => {
                    let (start_color, end_color) = if self.rainbow_mode_enabled {
                        let dx = (current_x - *start_x) as f64;
                        let dy = (current_y - *start_y) as f64;
                        let distance = (dx * dx + dy * dy).sqrt();
                        let start_hue = self.get_rainbow_hue();
                        let end_hue = start_hue + distance * self.rainbow_hue_step_per_pixel;
                        (Some(self.rainbow_color_from_hue(start_hue)),
                         Some(self.rainbow_color_from_hue(end_hue)))
                    } else {
                        (None, None)
                    };
                    Some(Shape::Arrow {
                        x1: *start_x,
                        y1: *start_y,
                        x2: current_x,
                        y2: current_y,
                        color: self.current_color,
                        thick: self.current_thickness,
                        arrow_length: self.arrow_length,
                        arrow_angle: self.arrow_angle,
                        start_color,
                        end_color,
                    })
                }
                Tool::Marker => Some(Shape::MarkerStroke {
                    points: points.clone(),
                    color: self.marker_color(),
                    thick: self.current_thickness,
                    per_point_colors: None,
                }),
                Tool::Eraser => None, // Preview handled separately to avoid clearing the buffer
                Tool::Highlight => None,
                Tool::Select => None,
                // No provisional shape for other tools
            }
        } else {
            None
        }
    }

    /// Renders the provisional shape directly to a Cairo context without cloning.
    ///
    /// This is an optimized version for freehand drawing that avoids cloning
    /// the points vector on every render, preventing quadratic performance.
    ///
    /// # Arguments
    /// * `ctx` - Cairo context to render to
    /// * `current_x` - Current mouse X coordinate
    /// * `current_y` - Current mouse Y coordinate
    ///
    /// # Returns
    /// `true` if a provisional shape was rendered, `false` otherwise
    pub fn render_provisional_shape(
        &self,
        ctx: &cairo::Context,
        current_x: i32,
        current_y: i32,
    ) -> bool {
        if let DrawingState::Drawing {
            tool,
            start_x: _,
            start_y: _,
            points,
        } = &self.state
        {
            match tool {
                Tool::Pen => {
                    // Render freehand without cloning - just borrow the points
                    let colors = if self.rainbow_mode_enabled {
                        Some(self.generate_rainbow_colors_for_points(points))
                    } else {
                        None
                    };
                    render_freehand_borrowed(
                        ctx,
                        points,
                        self.current_color,
                        self.current_thickness,
                        colors.as_deref(),
                    );
                    true
                }
                Tool::Highlight => false,
                Tool::Marker => {
                    let colors = if self.rainbow_mode_enabled {
                        Some(
                            self.generate_rainbow_colors_for_points(points)
                                .into_iter()
                                .map(|mut c| {
                                    c.a = self.marker_opacity;
                                    c
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    };
                    render_marker_stroke_borrowed(
                        ctx,
                        points,
                        self.marker_color(),
                        self.current_thickness,
                        colors.as_deref(),
                    );
                    true
                }
                Tool::Eraser => {
                    // Visual preview without actually clearing
                    let preview_color = Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 0.35,
                    };
                    render_freehand_borrowed(ctx, points, preview_color, self.eraser_size, None);
                    true
                }
                _ => {
                    // For other tools, use the normal path (no clone needed)
                    if let Some(shape) = self.get_provisional_shape(current_x, current_y) {
                        render_shape(ctx, &shape);
                        true
                    } else {
                        false
                    }
                }
            }
        } else {
            false
        }
    }

    pub(crate) fn marker_color(&self) -> Color {
        // Keep a minimum alpha so the marker remains visible even if a fully transparent color was set.
        let alpha = (self.current_color.a * self.marker_opacity).clamp(0.05, 0.9);
        Color {
            a: alpha,
            ..self.current_color
        }
    }
}
