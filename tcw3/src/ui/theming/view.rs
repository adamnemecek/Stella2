use alt_fp::FloatOrd;
use cggeom::box2;
use cgmath::Vector2;
use flags_macro::flags;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use super::{
    manager::{Elem, Manager, PropKindFlags},
    style::{ClassSet, ElemClassPath, Metrics, Prop, PropValue, Role},
};
use crate::{
    pal,
    pal::prelude::*,
    uicore::{HView, HWnd, Layout, LayoutCtx, SizeTraits, Sub, UpdateCtx, ViewFlags, ViewListener},
};

/// A box styled based on styling properties.
///
/// The following [`Prop`]s are handled: `NumLayers`, `LayerImg`,
/// `LayerBgColor`, `LayerMetrics`, `LayerOpacity`, `LayerCenter`, `LayerXform`,
/// `SubviewMetrics`, `ClipMetrics`, and `MinSize`.
///
/// [`Prop`]: crate::ui::theming::Prop
#[derive(Debug)]
pub struct StyledBox {
    view: HView,
    shared: Rc<Shared>,
}

#[derive(Debug)]
struct Shared {
    view: HView,

    style_elem: Elem,
    dirty: Cell<PropKindFlags>,

    subviews: RefCell<Vec<(Role, HView)>>,

    has_layer_group: bool,
}

impl StyledBox {
    pub fn new(style_manager: &'static Manager, view_flags: ViewFlags) -> Self {
        // Create `Elem` based on the inital properties
        let style_elem = Elem::new(style_manager);

        // Create the initial `Layout` based on the inital properties
        let subviews = Vec::new();
        let layout = SbLayout::new(&subviews, &style_elem);

        // Create and set up a `View`
        let view = HView::new(view_flags);

        let shared = Rc::new(Shared {
            view: view.clone(),
            subviews: RefCell::new(subviews),
            style_elem,
            // Already have an up-to-date `Layout`, so exclude it from
            // the dirty flags
            dirty: Cell::new(PropKindFlags::all() - PropKindFlags::LAYOUT),
            has_layer_group: view_flags.contains(ViewFlags::LAYER_GROUP),
        });

        view.set_listener(SbListener::new(Rc::downgrade(&shared)));
        view.set_layout(layout);

        // Get notified when the styling properties change
        {
            let shared_weak = Rc::downgrade(&shared);
            shared
                .style_elem
                .set_on_change(Box::new(move |_, kind_flags| {
                    if let Some(shared) = shared_weak.upgrade() {
                        shared.set_dirty(kind_flags);
                    }
                }));
        }

        Self { view, shared }
    }

    /// Set the class set of the styled element.
    pub fn set_class_set(&mut self, class_set: ClassSet) {
        self.shared.style_elem.set_class_set(class_set);
    }

    /// Set the parent class path.
    pub fn set_parent_class_path(&mut self, parent_class_path: Option<Rc<ElemClassPath>>) {
        self.shared
            .style_elem
            .set_parent_class_path(parent_class_path);
    }

    /// Set a subview for the specified `Role`.
    pub fn set_subview(&mut self, role: Role, view: Option<HView>) {
        let mut subviews = self.shared.subviews.borrow_mut();

        if let Some(view) = view {
            // Assign a subview
            if let Some(ent) = subviews.iter_mut().find(|(r, _)| *r == role) {
                ent.1 = view;
            } else {
                subviews.push((role, view));
            }
        } else {
            // Remove a subview
            if let Some(i) = subviews.iter().position(|(r, _)| *r == role) {
                subviews.remove(i);
            }
        }

        drop(subviews);

        // TODO: Add methods for deferring update
        self.shared.set_dirty(PropKindFlags::LAYOUT);
    }

    /// Get `Rc<ElemClassPath>` representing the class path of the styled
    /// element. The returned value can be set on subviews as a parent class
    /// path.
    pub fn class_path(&self) -> Rc<ElemClassPath> {
        self.shared.style_elem.class_path()
    }

    /// Get the view representing a styled box.
    pub fn view(&self) -> &HView {
        &self.view
    }
}

impl Shared {
    /// Dispatch update methods based on a `PropKindFlags`
    fn set_dirty(&self, diff: PropKindFlags) {
        if diff.intersects(PropKindFlags::LAYOUT) {
            self.view
                .set_layout(SbLayout::new(&self.subviews.borrow(), &self.style_elem));
        }

        if diff.intersects(flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]) {
            self.view.pend_update();
        }

        let dirty = &self.dirty;
        dirty.set((dirty.get() | diff) - PropKindFlags::LAYOUT);
    }
}

struct SbLayout {
    subview_layout: Vec<Metrics>,
    subviews: Vec<HView>,
    min_size: Vector2<f32>,
}

impl SbLayout {
    fn new(subviews: &Vec<(Role, HView)>, elem: &Elem) -> Self {
        // Evaluate the layout properties now
        Self {
            subview_layout: subviews
                .iter()
                .map(
                    |&(role, _)| match elem.compute_prop(Prop::SubviewMetrics(role)) {
                        PropValue::Metrics(m) => m,
                        _ => unreachable!(),
                    },
                )
                .collect(),
            subviews: subviews.iter().map(|x| x.1.clone()).collect(),
            min_size: match elem.compute_prop(Prop::MinSize) {
                PropValue::Vector2(v) => v,
                _ => unreachable!(),
            },
        }
    }
}

impl Layout for SbLayout {
    fn subviews(&self) -> &[HView] {
        &self.subviews
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let mut traits = SizeTraits {
            min: self.min_size,
            ..SizeTraits::default()
        };

        let mut num_pref_x = 0;
        let mut num_pref_y = 0;

        for (metrics, sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let margin = &metrics.margin;
            let sv_traits = ctx.subview_size_traits(sv);

            let margin_x = margin[1] + margin[3];
            let margin_y = margin[0] + margin[2];

            // For each axis, if two margins are fixed, the subview's `SizeTraits`
            // affects that of the superview
            if margin_x.is_finite() {
                traits.min.x = traits.min.x.fmax(sv_traits.min.x + margin_x);
                traits.max.x = traits.max.x.fmin(sv_traits.max.x + margin_x);
                traits.preferred.x += sv_traits.preferred.x + margin_x;
                num_pref_x += 1;
            }

            if margin_y.is_finite() {
                traits.min.y = traits.min.y.fmax(sv_traits.min.y + margin_y);
                traits.max.y = traits.max.y.fmin(sv_traits.max.y + margin_y);
                traits.preferred.y += sv_traits.preferred.y + margin_y;
                num_pref_y += 1;
            }
        }

        use std::cmp::max;

        traits.preferred.x /= max(num_pref_x, 1) as f32;
        traits.preferred.y /= max(num_pref_y, 1) as f32;

        traits
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        for (metrics, sv) in self.subview_layout.iter().zip(self.subviews.iter()) {
            let sv_traits = ctx.subview_size_traits(sv);
            let container = box2! {top_left: [0.0, 0.0], size: size};

            let frame = metrics.arrange(container, sv_traits.preferred);

            ctx.set_subview_frame(sv, frame);
        }
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        use as_any::Downcast;
        if let Some(other) = (*other).downcast_ref::<Self>() {
            self.subviews == other.subviews
        } else {
            false
        }
    }
}

struct SbListener {
    // Use a weak reference to break a cycle
    shared: Weak<Shared>,
    layers: RefCell<Option<Layers>>,
}

#[derive(Default)]
struct Layers {
    clip: Option<pal::HLayer>,
    styled: Vec<pal::HLayer>,
    sub: Option<Sub>,
}

impl SbListener {
    fn new(shared: Weak<Shared>) -> Self {
        Self {
            shared,
            layers: RefCell::new(None),
        }
    }
}

impl ViewListener for SbListener {
    fn mount(&self, wm: pal::WM, _: &HView, wnd: &HWnd) {
        let mut layers = self.layers.borrow_mut();
        assert!(layers.is_none());

        if let Some(shared) = self.shared.upgrade() {
            // Insert fake dirty flags to set the inital layer properties
            let dirty = &shared.dirty;
            dirty.set(dirty.get() | flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]);

            // Watch for DPI scale changes
            let sub = {
                let shared = self.shared.clone();
                wnd.subscribe_dpi_scale_changed(Box::new(move |_, _| {
                    if let Some(shared) = shared.upgrade() {
                        shared.set_dirty(PropKindFlags::LAYER_IMG);
                    }
                }))
            };

            // Create layers. Properties are set later in `update` (This happens
            // because of the fake dirty flags we inserted).
            *layers = Some(Layers {
                clip: if shared.has_layer_group {
                    Some(wm.new_layer(pal::LayerAttrs {
                        flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
                        ..pal::LayerAttrs::default()
                    }))
                } else {
                    None
                },
                styled: Vec::new(),
                sub: Some(sub),
            });
        } else {
            *layers = Some(Layers::default());
        }
    }

    fn unmount(&self, wm: pal::WM, _: &HView) {
        let layers = self.layers.borrow_mut().take().unwrap();

        for layer in layers.clip {
            wm.remove_layer(&layer);
        }
        for layer in layers.styled {
            wm.remove_layer(&layer);
        }

        if let Some(sub) = layers.sub {
            sub.unsubscribe().unwrap();
        }
    }

    fn position(&self, _: pal::WM, _: &HView) {
        if let Some(shared) = self.shared.upgrade() {
            shared.set_dirty(PropKindFlags::LAYER_BOUNDS);
        }
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let shared;
        if let Some(shared_rc) = self.shared.upgrade() {
            shared = shared_rc;
        } else {
            return;
        }

        let mut layers = self.layers.borrow_mut();
        let layers: &mut Layers = layers.as_mut().unwrap();

        let elem = &shared.style_elem;

        macro_rules! compute_prop {
            ($prop:expr, PropValue::$type:ident) => {
                match elem.compute_prop($prop) {
                    PropValue::$type(v) => v,
                    _ => unreachable!(),
                }
            };
        }

        let dirty = shared.dirty.get();
        shared
            .dirty
            .set(dirty - flags![PropKindFlags::{LAYER_ALL | CLIP_LAYER}]);

        // Adjust the layer count
        if dirty.intersects(PropKindFlags::NUM_LAYERS) {
            let num_layers = compute_prop!(Prop::NumLayers, PropValue::Usize);
            let styled = &mut layers.styled;

            while num_layers < styled.len() {
                wm.remove_layer(&styled.pop().unwrap());
            }
            styled.resize_with(num_layers, || wm.new_layer(pal::LayerAttrs::default()));
        }

        let container = view.global_frame();

        // Update layer properties
        let prop_flags = PropKindFlags::LAYER_ALL - PropKindFlags::NUM_LAYERS;
        if dirty.intersects(prop_flags) {
            for (i, layer) in layers.styled.iter().enumerate() {
                let layer_id = i as u32;
                let mut layer_attrs = pal::LayerAttrs::default();

                if dirty.intersects(PropKindFlags::LAYER_BOUNDS) {
                    let met = compute_prop!(Prop::LayerMetrics(layer_id), PropValue::Metrics);
                    let bounds = met.arrange(container, Vector2::new(0.0, 0.0));
                    layer_attrs.bounds = Some(bounds);
                }

                if dirty.intersects(PropKindFlags::LAYER_IMG) {
                    let img = compute_prop!(Prop::LayerImg(layer_id), PropValue::Himg);

                    if let Some(img) = img {
                        let (bmp, content_scale) = img.new_bmp(wm, ctx.hwnd().dpi_scale());

                        layer_attrs.contents = Some(Some(bmp));
                        layer_attrs.contents_scale = Some(content_scale);
                    } else {
                        layer_attrs.contents = Some(None);
                    }
                }

                if dirty.intersects(PropKindFlags::LAYER_BG_COLOR) {
                    let value = compute_prop!(Prop::LayerBgColor(layer_id), PropValue::Rgbaf32);
                    layer_attrs.bg_color = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_OPACITY) {
                    let value = compute_prop!(Prop::LayerOpacity(layer_id), PropValue::Float);
                    layer_attrs.opacity = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_CENTER) {
                    let value = compute_prop!(Prop::LayerCenter(layer_id), PropValue::Box2);
                    layer_attrs.contents_center = Some(value);
                }

                if dirty.intersects(PropKindFlags::LAYER_XFORM) {
                    let xform = compute_prop!(Prop::LayerXform(layer_id), PropValue::LayerXform);

                    let met = compute_prop!(Prop::LayerMetrics(layer_id), PropValue::Metrics);
                    let bounds = met.arrange(container, Vector2::new(0.0, 0.0));

                    let mat = xform.to_matrix3(bounds);

                    layer_attrs.transform = Some(mat);
                }

                wm.set_layer_attr(layer, layer_attrs);
            }
        }

        // Update the clip layer's properties
        if let Some(clip) = &layers.clip {
            if dirty.intersects(PropKindFlags::CLIP_LAYER) {
                let met = compute_prop!(Prop::ClipMetrics, PropValue::Metrics);

                let bounds = met.arrange(container, Vector2::new(0.0, 0.0));

                wm.set_layer_attr(
                    clip,
                    pal::LayerAttrs {
                        bounds: Some(bounds),
                        ..pal::LayerAttrs::default()
                    },
                );
            }
        }

        // Tell the system the layers we have
        let new_len = layers.styled.len() + (layers.clip.is_some() as usize);
        if ctx.layers().len() != new_len {
            let mut new_layers = layers.styled.clone();

            new_layers.extend(layers.clip.iter().cloned());

            ctx.set_layers(new_layers);
        }
    }
}
