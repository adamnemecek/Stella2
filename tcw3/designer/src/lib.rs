//! Declarative UI for TCW3
//!
//! Most parts of UI are static and imperative programming is not the best
//! option to write such things as it leads to an excessive amount of
//! boilerplate code. TCW3 Designer is a code generation framework that
//! addresses this issue.
//!
//! TCW3 designer is designed to meet the following requirements:
//!
//! - The structures of UI components can be expressed in a way that is mostly
//!   free of boilerplate code for procedurally constructing a structure.
//! - It generates widget controller types akin to standard widgets such as
//!   `tcw3::ui::views::Button` and they can be used in a similar way.
//! - Components in one crate can consume other components from another crate.
//! - Seamlessly integrates with existing components.
//!
//! # Usage
//!
//! TODO - please see `tcw3_meta`.
//!
//! # Implementation Details
//!
//! ## Crate Metadata
//!
//! ```text
//!                                                        tcw3_designer <-,
//!                                                                        |
//!  ,----------,     dep      ,---------------,  codegen  ,----------,    |
//!  | upstream | -----------> | upstream_meta | <-------- | build.rs | -> |
//!  '----------'              '---------------'           '----------'    |
//!       ^                            ^                         build-dep |
//!       |                            |       build-dep                   |
//!       | dep                        '------------------------,          |
//!       |                                                     |          |
//!       |                                                     |          |
//!  ,----------,     dep      ,---------------,  codegen  ,----------,    |
//!  | applicat | -----------> | applicat_meta | <-------- | build.rs | ---'
//!  '----------'              '---------------'           '----------'
//! ```
//!
//! In order to enable the consumption of other crate's components, TCW3
//! Designer makes use of build scripts. Each widget library crate has a meta
//! crate indicated by the suffix `_meta`. The source code of each meta crate
//! is generated by the build script, which can access other crates' information
//! by importing their meta crates through `build-dependencies`.
//!
//! ## Meta Crates
//!
//! Meta crates include a build script that uses [`BuildScriptConfig`] to
//! generate the source code of the crate. The generated code exports the
//! following two items:
//!
//! ```rust,no_compile
//! pub static DESIGNER_METADATA: &[u8] = [ /* ... */ ];
//! #[macro_export] macro_rules! designer_impl { /* ... */ }
//! ```
//!
//! `DESIGNER_METADATA` is encoded metadata, describing components and their
//! interfaces provided by the crate. You call [`BuildScriptConfig::link`] to
//! import `DESIGNER_METADATA` from another crate.
//!
//! `designer_impl` is used by the main crate to generate the skeleton
//! implementation for the defined components.
//!
//! ## Component Types
//!
//! For a `pub` component named `Component`, the following three types are
//! defined (they are inserted to a source file by `designer_impl` macro):
//!
//! ```rust,no_compile
//! pub struct Component {
//!     shared: Rc<ComponentShared>,
//! }
//!
//! struct ComponentShared {
//!     state: RefCell<ComponentState>,
//!     value_const1: u32,
//!     subscriptions_event1: RefCell<_>,
//!     /* ... */
//! }
//!
//! struct ComponentState {
//!     value_prop1: u32,
//!     value_wire1: u32,
//!     /* ...*/
//! }
//! ```
//!
//! ## Component Initialization
//!
//! **Field Initialization** —
//! The first step in the component constructor `Component::new` is to evaluate
//! the initial values of all fields and construct `ComponentState`,
//! `ComponentShared`, and then finally `Component`.
//!
//! A dependency graph is constructed. Each node represents one of the
//! following: (1) A field having a value, which is either an object
//! initialization literal `OtherComp { ... }` or a function `|dep| expr`.
//! (2) A `const` field in an object initialization literal in `Component`.
//! A topological order is found and the values are evaluated according to that.
//! Note that because none of the component's structs are available at this
//! point, **`this` cannot be used as an input to any of the fields** involved
//! here. Obviously, fields that are not initialized at this point cannot be
//! used as an input.
//!
//! **Props** —
//! The initial values of `prop` fields in object initialization literals are
//! initialized in a topological order found in a similar way.
//!
//! **Events** —
//! Event handlers are hooked up to child objects. `on (obj.event)` and
//! `on (obj.prop)` explicitly create event handlers. Props and wires with
//! functions like `|obj.prop| expr` register automatically-generated event
//! handlers for observing changes in the input values.
//!
//! The registration functions return `subscriber_list::UntypedSubscription`.
//! They are automatically unsubscribed when `Component` is dropped.
//!
//! Event handlers maintain weak references to `ComponentShared`.
//!
//! ## Updating State
//!
//! After dependencies are updated, recalculation (called *a commit operation*)
//! of props and wires is scheduled using `tcw3::uicore::WmExt::invoke_on_update`.
//! Since it's possible to borrow the values of props and wires anytime, the
//! callback function of `invoke_on_update` is the only place where the values
//! can be mutated reliably (though this is not guaranteed, so runtime checks
//! are still necessary for safety).
//! Most importantly, even the effect of prop setters are deferred in this way.
//! New prop values are stored in a separate location until they are assigned
//! during a commit operation.
//!
//! A bit array is used as dirty flags for tracking which fields need to be
//! recalculated. Basically, each prop and wire with a functional value receives
//! a dirty flag. (TODO: Optimize dirty flag mapping and propagation)
//!
mod codegen;
mod metadata;

pub use self::codegen::BuildScriptConfig;
