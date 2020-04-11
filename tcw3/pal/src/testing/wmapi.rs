use cgmath::{Point2, Vector2};
use std::time::Instant;

use crate::{iface, HTextInputCtx, HWnd};

/// Provides access to a virtual environment.
///
/// This is provided as a trait so that testing code can be compiled even
/// without a `testing` feature flag.
pub trait TestingWm: 'static {
    /// Get the global instance of [`tcw3::pal::Wm`]. This is identical to
    /// calling `Wm::global()`.
    ///
    /// [`tcw3::pal::Wm`]: crate::Wm
    fn wm(&self) -> crate::Wm;

    /// Process events until all `!Send` dispatches (those generated by
    /// `Wm::invoke`, but not `Wm::invoke_on_main_thread`) are processed.
    fn step_unsend(&self);

    /// Process events until at least one event is processed.
    fn step(&self);

    /// Process events until at least one event is processed or
    /// until the specified instant.
    fn step_until(&self, till: Instant);

    /// Get a list of currently open windows.
    fn hwnds(&self) -> Vec<HWnd>;

    /// Get the attributes of a window.
    fn wnd_attrs(&self, hwnd: &HWnd) -> Option<WndAttrs>;

    /// Trigger `WndListener::close_requested`.
    fn raise_close_requested(&self, hwnd: &HWnd);

    /// Set a given window's DPI scale and trigger
    /// `WndListener::dpi_scale_changed`.
    ///
    /// `dpi_scale` must be positive and finite.
    ///
    /// TODO: Add a method to set the default DPI scale
    fn set_wnd_dpi_scale(&self, hwnd: &HWnd, dpi_scale: f32);

    /// Set a given window's size and trigger `WndListener::resize`.
    ///
    /// `size` is not automatically clipped by `min_size` or `max_size`.
    fn set_wnd_size(&self, hwnd: &HWnd, size: [u32; 2]);

    /// Set the focus state of a given window and trigger `WndListener::focus`.
    fn set_wnd_focused(&self, hwnd: &HWnd, focused: bool);

    /// Render the content of a given window and update `out` with it.
    fn read_wnd_snapshot(&self, hwnd: &HWnd, out: &mut WndSnapshot);

    /// Trigger `WndListener::mouse_motion`.
    fn raise_mouse_motion(&self, hwnd: &HWnd, loc: Point2<f32>);

    /// Trigger `WndListener::mouse_leave`.
    fn raise_mouse_leave(&self, hwnd: &HWnd);

    /// Trigger `WndListener::mouse_drag`.
    fn raise_mouse_drag(&self, hwnd: &HWnd, loc: Point2<f32>, button: u8) -> Box<dyn MouseDrag>;

    // TODO: `WndListener::nc_hit_test`

    /// Trigger `WndListener::scroll_motion`.
    fn raise_scroll_motion(&self, hwnd: &HWnd, loc: Point2<f32>, delta: &iface::ScrollDelta);

    /// Trigger `WndListener::scroll_gesture`.
    fn raise_scroll_gesture(&self, hwnd: &HWnd, loc: Point2<f32>) -> Box<dyn ScrollGesture>;

    /// Get the list of currently active text input contexts.
    fn active_text_input_ctxs(&self) -> Vec<HTextInputCtx>;

    /// Get the currently active text input context. Panic if there are more
    /// than one of such contexts.
    fn expect_unique_active_text_input_ctx(&self) -> Option<HTextInputCtx>;

    /// Trigger `TextInputCtxListener::edit`.
    fn raise_edit(
        &self,
        htictx: &HTextInputCtx,
        write: bool,
    ) -> Box<dyn iface::TextInputCtxEdit<crate::Wm>>;
}

/// A snapshot of window attributes.
#[derive(Debug, Clone)]
pub struct WndAttrs {
    pub size: [u32; 2],
    pub min_size: [u32; 2],
    pub max_size: [u32; 2],
    pub flags: iface::WndFlags,
    pub caption: String,
    pub visible: bool,
    pub cursor_shape: iface::CursorShape,
}

/// Provides an interface for simulating a mouse drag geature.
///
/// See [`MouseDragListener`] for the semantics of the methods.
///
/// [`MouseDragListener`]: crate::iface::MouseDragListener
pub trait MouseDrag {
    /// Trigger `MouseDragListener::mouse_motion`.
    fn mouse_motion(&self, _loc: Point2<f32>);
    /// Trigger `MouseDragListener::mouse_down`.
    fn mouse_down(&self, _loc: Point2<f32>, _button: u8);
    /// Trigger `MouseDragListener::mouse_up`.
    fn mouse_up(&self, _loc: Point2<f32>, _button: u8);
    /// Trigger `MouseDragListener::cancel`.
    fn cancel(&self);
}

/// Provides an interface for simulating a scroll geature.
///
/// See [`ScrollListener`] for the semantics of the methods.
///
/// [`ScrollListener`]: crate::iface::ScrollListener
pub trait ScrollGesture {
    /// Trigger `ScrollListener::mouse_motion`.
    fn motion(&self, delta: &iface::ScrollDelta, velocity: Vector2<f32>);
    /// Trigger `ScrollListener::mouse_down`.
    fn start_momentum_phase(&self);
    /// Trigger `ScrollListener::mouse_up`.
    fn end(&self);
    /// Trigger `ScrollListener::cancel`.
    fn cancel(&self);
}

/// An RGBA8 image created from the contents of a window.
#[derive(Debug, Clone, Default)]
pub struct WndSnapshot {
    /// The size of the image.
    pub size: [usize; 2],
    /// Image data.
    pub data: Vec<u8>,
    /// The byte offset between adjacent rows.
    pub stride: usize,
}

impl WndSnapshot {
    /// Create an empty `WndSnapshot`.
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActionBinding {
    pub source: &'static str,
    pub pattern: &'static str,
    pub action: iface::ActionId,
}
