use arrayvec::ArrayVec;
use bitflags::bitflags;
use iterpool::{intrusive_list, IterablePool, PoolPtr};
use sorted_diff::{sorted_diff, In};
use std::{
    cell::{Cell, RefCell},
    fmt,
};
use subscriber_list::{SubscriberList, UntypedSubscription as Sub};
use tcw3_pal::mt_lazy_static;

use super::{
    style::{ClassSet, ElemClassPath, Prop, PropValue},
    stylesheet::{DefaultStylesheet, RuleId, Stylesheet},
};
use crate::{pal, prelude::*};

pub(crate) type SheetId = usize;

pub type ManagerNewSheetSetCb = Box<dyn Fn(pal::Wm, &Manager, &mut NewSheetSetCtx<'_>)>;

/// The maxiumum supported depth of styling element hierarchy.
pub const MAX_ELEM_DEPTH: usize = crate::uicore::MAX_VIEW_DEPTH;

/// The center of the theming system.
///
/// `Manager` stores the currently active stylesheet set ([`SheetSet`]), which
/// is usually applied to entire the application. When it's changed, it sends
/// out a notification via the callback functions registered via
/// `subscribe_sheet_set_changed`.
pub struct Manager {
    wm: pal::Wm,
    sheet_set: RefCell<SheetSet>,
    new_set_handlers: RefCell<SubscriberList<ManagerNewSheetSetCb>>,
    elems: RefCell<IterablePool<ElemInner>>,
    /// Use `dirty_list_accessor` to interact with this linked list.
    dirty_elems: Cell<intrusive_list::ListHead>,
    refresh_scheduled: Cell<bool>,
    sheet_set_invalidated: Cell<bool>,
    /// Renewed on every refresh. Used to ensure no elements are recalculated
    /// more than once per refresh.
    refresh_token: Cell<u64>,
}

impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Manager")
            .field("wm", &self.wm)
            .field("sheet_set", &())
            .field("set_change_handlers", &())
            .field("new_set_handlers", &())
            .field("elems", &self.elems)
            .field("dirty_elems", &self.dirty_elems)
            .field("refresh_scheduled", &self.refresh_scheduled)
            .field("sheet_set_invalidated", &self.sheet_set_invalidated)
            .field("refresh_token", &self.refresh_token)
            .finish()
    }
}

mt_lazy_static! {
    static ref GLOBAL_MANAGER: Manager => Manager::new;
}

/// Construct a `ListAccessorCell` that can be used to interact with
/// the children list of an `Elem`.
macro_rules! dirty_list_accessor {
    ($manager:expr, $elems:expr) => {
        intrusive_list::ListAccessorCell::new(&$manager.dirty_elems, $elems, |e: &ElemInner| {
            &e.dirty_link
        })
    };
}

/// Construct a `ListAccessorCell` that can be used to interact with
/// the children list of an `Elem`.
macro_rules! child_accessor {
    ($head:expr, $elems:expr) => {
        intrusive_list::ListAccessorCell::new($head, $elems, |e: &ElemInner| &e.sibling)
    };
}

type ElemClassPathBuf = ArrayVec<[ClassSet; MAX_ELEM_DEPTH]>;

impl Manager {
    fn new(wm: pal::Wm) -> Self {
        let this = Self {
            wm,
            sheet_set: RefCell::new(SheetSet { sheets: Vec::new() }),
            new_set_handlers: RefCell::new(SubscriberList::new()),
            elems: RefCell::new(IterablePool::new()),
            dirty_elems: Cell::new(intrusive_list::ListHead::new()),
            refresh_scheduled: Cell::new(false),
            sheet_set_invalidated: Cell::new(false),
            refresh_token: Cell::new(0),
        };

        // Create the first `SheetSet`
        let sheet_set = this.new_sheet_set();
        *this.sheet_set.borrow_mut() = sheet_set;

        this
    }

    /// Get a global instance of `Manager`.
    pub fn global(wm: pal::Wm) -> &'static Self {
        GLOBAL_MANAGER.get_with_wm(wm)
    }

    /// Register a callback function called when a new stylesheet set is being
    /// created.
    ///
    /// The specified function is called when the stylesheet is updated for the
    /// next time, i.e., when the operating system's apperance setting is
    /// updated or `update_sheet_set` is called.
    pub fn subscribe_new_sheet_set(&self, cb: ManagerNewSheetSetCb) -> Sub {
        self.new_set_handlers.borrow_mut().insert(cb).untype()
    }

    /// Force the recreation the stylesheet set.
    pub fn update_sheet_set(&'static self) {
        let sheet_set = self.new_sheet_set();
        *self.sheet_set.borrow_mut() = sheet_set;

        self.sheet_set_invalidated.set(true);

        // All elements are to be recalculated
        let elems = self.elems.borrow();
        for (ptr, el) in elems.ptr_iter() {
            if el.parent.get().is_some() {
                // Children are transitively scanned when their parents have
                // a dirty flag, so there's no point in adding the children to
                // athe
                continue;
            }

            add_elem_to_dirty_list(self, ptr, &*elems);
        }

        self.schedule_refresh();
    }

    /// Construct a new `SheetSet` using the default stylesheet and
    /// `new_set_handlers`.
    fn new_sheet_set(&self) -> SheetSet {
        let mut sheet_set = SheetSet {
            sheets: vec![Box::new(DefaultStylesheet)],
        };

        for handler in self.new_set_handlers.borrow().iter() {
            handler(
                self.wm,
                self,
                &mut NewSheetSetCtx {
                    sheet_set: &mut sheet_set,
                },
            );
        }

        sheet_set
    }

    /// Get the currently active sheet set.
    ///
    /// This may change throughout the application's lifecycle. Use
    /// `subscribe_sheet_set_changed` to get notified when it happens.
    pub(crate) fn sheet_set<'a>(&'a self) -> impl std::ops::Deref<Target = SheetSet> + 'a {
        self.sheet_set.borrow()
    }

    #[inline]
    fn schedule_refresh(&'static self) {
        if !self.refresh_scheduled.get() {
            self.schedule_refresh_inner();
        }
    }

    fn schedule_refresh_inner(&'static self) {
        self.refresh_scheduled.set(true);
        self.wm.invoke_on_update(move |_| {
            self.refresh_scheduled.set(false);
            self.refresh();
        });
    }

    fn refresh(&self) {
        self.refresh_token.set(
            self.refresh_token
                .get()
                .checked_add(1)
                .expect("refresh token exhausted"),
        );

        let elems = self.elems.borrow();
        let dirty_list = dirty_list_accessor!(self, &*elems);

        let sheet_set = self.sheet_set.borrow();

        let mut path = ElemClassPathBuf::new();
        while let Some(ptr) = dirty_list.pop_front() {
            elems[ptr].dirty.set(false);
            elem_get_class_path(ptr, &*elems, &mut path);
            self.refresh_traverse(ptr, &*elems, &mut path, &sheet_set);
        }

        self.sheet_set_invalidated.set(false);
    }

    fn refresh_traverse(
        &self,
        elem_ptr: PoolPtr,
        elems: &IterablePool<ElemInner>,
        path: &mut ElemClassPathBuf,
        sheet_set: &SheetSet,
    ) {
        let el = &elems[elem_ptr];
        if el.refresh_token.get() == self.refresh_token.get() {
            // The element has already been recalculated in this round
            return;
        }
        el.refresh_token.set(self.refresh_token.get());

        // Update the active rule set
        let mut rules = el.rules.borrow_mut();
        let sheet_set_invalidated = self.sheet_set_invalidated.get();
        let diff = rules.update(&sheet_set, &path, sheet_set_invalidated);
        drop(rules);

        // Notify if there are any changes
        if !diff.is_empty() {
            el.change_handler.borrow()(self.wm, diff);
        }

        // Scan children
        let child_list = child_accessor!(&el.children, elems);
        for (child_ptr, child_el) in child_list.iter() {
            path.push(child_el.class_set.get());
            self.refresh_traverse(child_ptr, elems, path, sheet_set);
            path.pop();
        }
    }
}

/// The context type passed to callback functions of type [`ManagerNewSheetSetCb`].
pub struct NewSheetSetCtx<'a> {
    sheet_set: &'a mut SheetSet,
}

impl NewSheetSetCtx<'_> {
    /// Insert a new `Stylesheet`.
    pub fn insert_stylesheet(&mut self, stylesheet: impl Stylesheet + 'static) {
        self.sheet_set.sheets.push(Box::new(stylesheet));
    }
}

/// A stylesheet set.
pub(crate) struct SheetSet {
    sheets: Vec<Box<dyn Stylesheet>>,
}

impl SheetSet {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(SheetId, RuleId)) {
        for (i, sheet) in self.sheets.iter().enumerate() {
            sheet.match_rules(path, &mut |rule_id| out_rules(i, rule_id));
        }
    }

    fn get_rule(&self, id: (SheetId, RuleId)) -> Option<Rule<'_>> {
        self.sheets.get(id.0).and_then(|stylesheet| {
            if stylesheet.get_rule_priority(id.1).is_some() {
                Some(Rule {
                    stylesheet: &**stylesheet,
                    rule_id: id.1,
                })
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Copy)]
struct Rule<'a> {
    stylesheet: &'a dyn Stylesheet,
    rule_id: RuleId,
}

impl Rule<'_> {
    fn priority(&self) -> i32 {
        self.stylesheet.get_rule_priority(self.rule_id).unwrap()
    }
    fn prop_kinds(&self) -> PropKindFlags {
        self.stylesheet.get_rule_prop_kinds(self.rule_id).unwrap()
    }
    fn get_prop_value(&self, prop: &Prop) -> Option<&PropValue> {
        self.stylesheet
            .get_rule_prop_value(self.rule_id, prop)
            .unwrap()
    }
}

bitflags! {
    /// Represents categories of updated styling properties.
    ///
    /// When an `Elem`'s class set is updated, we must figure out which
    /// properties have to be recomputed. It would be inefficient to precisely
    /// track every property, so we categorize the properties into coarse groups
    /// and track changes in this unit.
    pub struct PropKindFlags: u16 {
        const NUM_LAYERS = 1;
        const LAYER_IMG = 1 << 1;
        const LAYER_BOUNDS = 1 << 2;
        const LAYER_BG_COLOR = 1 << 3;
        const LAYER_OPACITY = 1 << 4;
        const LAYER_CENTER = 1 << 5;
        const LAYER_XFORM = 1 << 6;
        /// Any properties of decorative layers.
        const LAYER_ALL = Self::NUM_LAYERS.bits |
            Self::LAYER_IMG.bits |
            Self::LAYER_BOUNDS.bits |
            Self::LAYER_BG_COLOR.bits |
            Self::LAYER_OPACITY.bits |
            Self::LAYER_CENTER.bits |
            Self::LAYER_XFORM.bits;
        const CLIP_LAYER = 1 << 7;
        const LAYOUT = 1 << 8;
        const FONT = 1 << 9;
        const FG_COLOR = 1 << 10;
    }
}

impl Prop {
    pub fn kind_flags(&self) -> PropKindFlags {
        // Make sure to synchronize these with the `prop!` macro - This is a
        // temporary restriction until `match` inside `const fn` is implemented:
        // <https://github.com/rust-lang/rust/issues/49146>
        match *self {
            Prop::NumLayers => PropKindFlags::LAYER_ALL,
            Prop::LayerImg(_) => PropKindFlags::LAYER_IMG,
            Prop::LayerBgColor(_) => PropKindFlags::LAYER_BG_COLOR,
            Prop::LayerMetrics(_) => PropKindFlags::LAYER_BOUNDS,
            Prop::LayerOpacity(_) => PropKindFlags::LAYER_OPACITY,
            Prop::LayerCenter(_) => PropKindFlags::LAYER_CENTER,
            Prop::LayerXform(_) => PropKindFlags::LAYER_XFORM,
            Prop::SubviewMetrics(_) => PropKindFlags::LAYOUT,
            Prop::MinSize => PropKindFlags::LAYOUT,
            Prop::ClipMetrics => PropKindFlags::CLIP_LAYER,
            Prop::FgColor => PropKindFlags::FG_COLOR,
            Prop::Font => PropKindFlags::FONT,
        }
    }
}

/// Represents a styled element.
///
/// This type tracks the currently-active rule set of a styled element. It
/// subscribes to [`Manager`]'s sheet set change handler and automatically
/// updates the active rule set whenever the sheet set is changed. It tracks
/// changes in properties, and calls the provided [`ElemChangeCb`] whenever
/// styling properties of the corresponding styled element are updated.
pub struct Elem {
    style_manager: &'static Manager,
    ptr: PoolPtr,
}

impl fmt::Debug for Elem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let inner = self.inner();

        f.debug_struct("Elem").field("inner", &*inner).finish()
    }
}

/// Identifies an instance of [`Elem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HElem {
    ptr: PoolPtr,
}

pub type ElemChangeCb = Box<dyn Fn(pal::Wm, PropKindFlags)>;

struct ElemInner {
    class_set: Cell<ClassSet>,
    rules: RefCell<ElemRules>,
    /// The function called when property values might have changed.
    change_handler: RefCell<ElemChangeCb>,

    parent: Cell<Option<PoolPtr>>,
    /// Use `child_accessor` to interact with this linked list.
    children: Cell<intrusive_list::ListHead>,
    /// Used by `child_accessor`
    sibling: Cell<Option<intrusive_list::Link>>,

    /// Used by `dirty_list_accessor`
    dirty_link: Cell<Option<intrusive_list::Link>>,
    dirty: Cell<bool>,
    refresh_token: Cell<u64>,
}

#[derive(Debug)]
struct ElemRules {
    // Currently-active rules, sorted by a lexicographical order.
    rules_by_ord: Vec<(SheetId, RuleId)>,
    // Currently-active rules, sorted by an ascending order of priority.
    rules_by_prio: Vec<(SheetId, RuleId)>,
}

impl fmt::Debug for ElemInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ElemInner")
            .field("class_set", &self.class_set)
            .field("rules", &self.rules)
            .field("change_handler", &((&self.change_handler) as *const _))
            .finish()
    }
}

impl Drop for Elem {
    fn drop(&mut self) {
        let mut elems = self.style_manager.elems.borrow_mut();
        let elems = &mut *elems; // enable split borrow

        // Remove from the parent
        let this_el = &elems[self.ptr];
        if let Some(ptr) = this_el.parent.get() {
            let parent_el = &elems[ptr];
            child_accessor!(&parent_el.children, &*elems).remove(self.ptr);
        }

        // Remove `self` from the dirty element list
        if this_el.dirty.get() {
            dirty_list_accessor!(self.style_manager, &*elems).remove(self.ptr);
        }

        // Add all children to the dirty element list
        for (ptr, child) in child_accessor!(&this_el.children, &*elems).iter() {
            add_elem_to_dirty_list(self.style_manager, ptr, &*elems);

            debug_assert_eq!(child.parent.get(), Some(self.ptr));
            child.parent.set(None);
        }

        // Remove all children
        child_accessor!(&this_el.children, &*elems).clear();

        // Schedule a refresh because dirty flags might have been set for some
        // elements
        self.style_manager.schedule_refresh();

        elems.deallocate(self.ptr).unwrap();
    }
}

impl Elem {
    /// Construct an `Elem`.
    pub fn new(style_manager: &'static Manager) -> Self {
        let inner = ElemInner {
            class_set: Cell::new(ClassSet::empty()),
            rules: RefCell::new(ElemRules {
                rules_by_ord: Vec::new(),
                rules_by_prio: Vec::new(),
            }),
            change_handler: RefCell::new(Box::new(|_, _| {})),

            parent: Cell::new(None),
            children: Cell::new(intrusive_list::ListHead::new()),
            sibling: Cell::new(None),

            dirty_link: Cell::new(None),
            dirty: Cell::new(false),
            refresh_token: Cell::new(0),
        };

        let ptr = style_manager.elems.borrow_mut().allocate(inner);

        add_elem_to_dirty_list(style_manager, ptr, &style_manager.elems.borrow());
        style_manager.schedule_refresh();

        Self { style_manager, ptr }
    }

    /// Set a callback function called when property values might have changed.
    ///
    /// It's prohibited to make any sorts of changes to any styling elements
    /// in the callback function.
    pub fn set_on_change(&self, handler: ElemChangeCb) {
        *self.inner().change_handler.borrow_mut() = handler;
    }

    /// Get the computed value of the specified styling property.
    pub fn compute_prop(&self, prop: Prop) -> PropValue {
        let manager = self.style_manager;
        let sheet_set = manager.sheet_set();
        self.inner().rules.borrow().compute_prop(&sheet_set, prop)
    }

    /// Set the class set and update the active rule set.
    ///
    /// This might internally call the `ElemChangeCb` registered by
    /// `set_on_change`.
    pub fn set_class_set(&self, class_set: ClassSet) {
        let elems = self.style_manager.elems.borrow();
        let el = &elems[self.ptr];
        el.class_set.set(class_set);

        add_elem_to_dirty_list(self.style_manager, self.ptr, &*elems);
        self.style_manager.schedule_refresh();
    }

    /// Get the class set.
    pub fn class_set(&self) -> ClassSet {
        self.inner().class_set.get()
    }

    /// Get the handle to this `Elem`. The handle is only valid as long as
    /// `self` lives.
    pub fn helem(&self) -> HElem {
        HElem { ptr: self.ptr }
    }

    /// Insert a child element. If `child` already belongs to another element,
    /// it will be removed first.
    pub fn insert_child(&self, child: HElem) {
        let elems = self.style_manager.elems.borrow();
        let child_el = &elems[child.ptr];

        // Remove `child` from its parent (if any) first
        if let Some(ptr) = child_el.parent.get() {
            let parent_el = &elems[ptr];
            child_accessor!(&parent_el.children, &*elems).remove(child.ptr);
        }

        let this_el = &elems[self.ptr];
        child_accessor!(&this_el.children, &*elems).push_back(child.ptr);

        child_el.parent.set(Some(self.ptr));

        add_elem_to_dirty_list(self.style_manager, child.ptr, &*elems);
        self.style_manager.schedule_refresh();
    }

    /// Remove a child element. Returns `true` iff `child` was found as a child
    /// element of `self` and removed.
    pub fn remove_child(&self, child: HElem) -> bool {
        let elems = self.style_manager.elems.borrow();
        let child_el = &elems[child.ptr];
        let this_el = &elems[self.ptr];

        if child_el.parent.get() == Some(self.ptr) {
            child_accessor!(&this_el.children, &*elems).remove(child.ptr);

            add_elem_to_dirty_list(self.style_manager, child.ptr, &*elems);
            self.style_manager.schedule_refresh();

            child_el.parent.set(None);
            true
        } else {
            false
        }
    }

    fn inner(&self) -> impl std::ops::Deref<Target = ElemInner> {
        use owning_ref::OwningRef;
        OwningRef::new(self.style_manager.elems.borrow()).map(|elems| &elems[self.ptr])
    }
}

/// Add `self` to the dirty element list.
fn add_elem_to_dirty_list(style_manager: &Manager, ptr: PoolPtr, elems: &IterablePool<ElemInner>) {
    let this_el = &elems[ptr];
    if !this_el.dirty.get() {
        this_el.dirty.set(true);
        dirty_list_accessor!(style_manager, elems).push_back(ptr);
    }
}

fn elem_get_class_path(
    mut ptr: PoolPtr,
    elems: &IterablePool<ElemInner>,
    out: &mut ElemClassPathBuf,
) {
    out.clear();

    while let Some(next) = {
        let el = &elems[ptr];
        out.push(el.class_set.get());
        el.parent.get()
    } {
        ptr = next;
    }

    out.reverse();
}

impl ElemRules {
    /// Get the computed value of the specified styling property.
    fn compute_prop(&self, sheet_set: &SheetSet, prop: Prop) -> PropValue {
        let mut computed_value = PropValue::default_for_prop(&prop);
        let kind = prop.kind_flags();

        for &id in self.rules_by_prio.iter() {
            let rule = sheet_set.get_rule(id).unwrap();
            if rule.prop_kinds().intersects(kind) {
                if let Some(specified_value) = rule.get_prop_value(&prop) {
                    computed_value = specified_value.clone();
                }
            }
        }

        computed_value
    }

    /// Recalculate the active rule set.
    ///
    /// This method assumes that the stylesheet set haven't changed since the
    /// last time the active rule set was calculated. If it has changed,
    /// `invalidate` must be set to `true`.
    ///
    /// Returns `PropKindFlags` indicating which property might have been
    /// changed.
    fn update(
        &mut self,
        sheet_set: &SheetSet,
        class_path: &ElemClassPath,
        invalidate: bool,
    ) -> PropKindFlags {
        let mut new_rules = Vec::with_capacity(self.rules_by_ord.len());
        sheet_set.match_rules(class_path, &mut |sheet_id, rule_id| {
            new_rules.push((sheet_id, rule_id));
        });
        new_rules.sort_unstable();

        // Calculate `PropKindFlags`
        let mut flags;

        if invalidate {
            flags = PropKindFlags::all();
        } else {
            flags = PropKindFlags::empty();

            for diff in sorted_diff(self.rules_by_ord.iter(), new_rules.iter()) {
                match diff {
                    In::Left(&id) | In::Right(&id) => {
                        flags |= sheet_set.get_rule(id).unwrap().prop_kinds();
                    }
                    In::Both(_, _) => {}
                }
            }

            if flags.is_empty() {
                // No changes
                return flags;
            }
        }

        self.rules_by_ord = new_rules;
        self.update_rules_by_prio(sheet_set);

        flags
    }

    /// Update `self.rules_by_prio` based on `self.rules_by_ord`.
    fn update_rules_by_prio(&mut self, sheet_set: &SheetSet) {
        self.rules_by_prio.clear();
        self.rules_by_prio.extend(self.rules_by_ord.iter().cloned());
        self.rules_by_prio
            .sort_unstable_by_key(|id| sheet_set.get_rule(*id).unwrap().priority());
    }
}
