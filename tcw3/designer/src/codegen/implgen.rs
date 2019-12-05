//! Implementation code generation
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use either::{Left, Right};
use quote::ToTokens;
use std::{collections::HashMap, fmt, fmt::Write, iter::repeat};

use super::{diag::Diag, sem};
use crate::metadata;

mod iterutils;
use self::iterutils::Iterutils;

/// Paths to standard library items.
mod paths {
    pub const BOX: &str = "::std::boxed::Box";
    pub const OPTION: &str = "::std::option::Option";
    pub const SOME: &str = "::std::option::Option::Some";
    pub const RC: &str = "::std::rc::Rc";
    pub const CELL: &str = "::std::cell::Cell";
    pub const REF_CELL: &str = "::std::cell::RefCell";
    pub const FN: &str = "::std::ops::Fn";
    pub const SUB_LIST: &str = "::tcw3::designer_runtime::SubscriberList";
    pub const UNSET: &str = "::tcw3::designer_runtime::Unset";
}

mod fields {
    pub const SHARED: &str = "shared";
    pub const STATE: &str = "state";
}

pub struct Ctx {
    /// The list of loaded crates. `[0]` is always the current crate.
    pub crates: Vec<metadata::Crate>,

    /// Mapping from external crate names to indices into `crates`.
    pub crate_map: HashMap<String, usize>,
}

pub fn gen_comp(
    comp: &sem::CompDef<'_>,
    meta_comp: &metadata::CompDef,
    _ctx: &Ctx,
    diag: &mut Diag,
) -> String {
    if comp.flags.contains(sem::CompFlags::PROTOTYPE_ONLY) {
        return r#"compile_error!(
            "`designer_impl!` can't generate code because the component is defined with #[prototype_only]"
        )"#.to_string();
    }

    let mut out = String::new();

    let comp_ident = &comp.path.segments.last().unwrap().ident;

    // String → index into `comp.items`
    // This also checks duplicate item names.
    let _item_name_map = make_name_map(
        comp.items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                sem::CompItemDef::Field(item) => Some((i, item.ident.clone())),
                sem::CompItemDef::Event(item) => Some((i, item.ident.clone())),
                sem::CompItemDef::On(_) => None,
            }),
        diag,
    );

    // `struct ComponentType`
    // -------------------------------------------------------------------
    writeln!(out, "#![derive(Clone)]").unwrap();
    writeln!(
        out,
        "{vis} struct {comp} {{",
        vis = comp.vis.to_token_stream(),
        comp = comp_ident
    )
    .unwrap();
    writeln!(
        out,
        "    {field}: {rc}<{comp}Shared>,",
        field = fields::SHARED,
        rc = paths::RC,
        comp = comp_ident
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeShared`
    // -------------------------------------------------------------------
    writeln!(out, "struct {}Shared {{", comp_ident).unwrap();
    writeln!(
        out,
        "    {field}: {cell}<{comp}State>,",
        field = fields::STATE,
        cell = paths::REF_CELL,
        comp = comp_ident
    )
    .unwrap();

    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {
                    writeln!(
                        out,
                        "    {ident}: {ty},",
                        ident = InnerValueField(&item.ident.sym),
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
                sem::FieldType::Wire => {}
                sem::FieldType::Prop => {
                    // TODO
                }
            },
            sem::CompItemDef::Event(item) => {
                writeln!(
                    out,
                    "    {ident}: {cell}<{list}<{handler}>>,",
                    ident = EventInnerSubList(&item.ident.sym),
                    cell = paths::REF_CELL,
                    list = paths::SUB_LIST,
                    handler = EventBoxHandlerTy(&item.inputs)
                )
                .unwrap();
            }
            sem::CompItemDef::On(_) => {
                // TODO
            }
        }
    }

    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeState`
    // -------------------------------------------------------------------
    writeln!(out, "struct {}State {{", comp_ident).unwrap();
    for item in comp.items.iter() {
        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {}
                sem::FieldType::Wire | sem::FieldType::Prop => {
                    writeln!(
                        out,
                        "    {ident}: {ty},",
                        ident = InnerValueField(&item.ident.sym),
                        ty = item.ty.to_token_stream()
                    )
                    .unwrap();
                }
            },
            sem::CompItemDef::Event(_) => {}
            sem::CompItemDef::On(_) => {}
        }
    }
    writeln!(out, "}}").unwrap();

    // `struct ComponentTypeBuilder`
    // -------------------------------------------------------------------
    let builder_vis = meta_comp.builder_vis();

    let settable_consts = comp.items.iter().filter_map(|item| match item {
        sem::CompItemDef::Field(
            field @ sem::FieldDef {
                field_ty: sem::FieldType::Const,
                accessors: sem::FieldAccessors { set: Some(_), .. },
                ..
            },
        ) => Some(field),
        _ => None,
    });
    let optional_consts = settable_consts
        .clone()
        .filter(|field| field.value.is_some());
    let non_optional_consts = settable_consts
        .clone()
        .filter(|field| field.value.is_none());
    let num_non_optional_consts = non_optional_consts.clone().count();

    // `T_field1`, `T_field2`, ...
    let builder_ty_params = non_optional_consts
        .clone()
        .map(|field| FactoryGenParamNameForField(&field.ident.sym));

    writeln!(
        out,
        "{vis} struct {comp}Builder{gen} {{",
        vis = builder_vis,
        comp = comp_ident,
        gen = if num_non_optional_consts != 0 {
            Left(Angle(CommaSeparated(builder_ty_params.clone())))
        } else {
            Right("")
        }
    )
    .unwrap();
    for field in settable_consts.clone() {
        writeln!(
            out,
            "    {ident}: {ty},",
            ident = InnerValueField(&field.ident.sym),
            ty = if field.value.is_some() {
                // optional
                Left(format!("{}<{}>", paths::OPTION, field.ty.to_token_stream()))
            } else {
                // non-optional - use a generics-based trick to enforce
                //                initialization
                Right(FactoryGenParamNameForField(&field.ident.sym))
            },
        )
        .unwrap();
    }
    writeln!(out, "}}").unwrap();

    // `ComponentBuilder::<Unset, ...>::new`
    writeln!(
        out,
        "impl {comp}Builder{gen} {{",
        comp = comp_ident,
        gen = if num_non_optional_consts != 0 {
            Left(Angle(CommaSeparated(
                repeat(paths::UNSET).take(num_non_optional_consts),
            )))
        } else {
            Right("")
        }
    )
    .unwrap();
    writeln!(out, "    {vis} fn new() -> Self {{", vis = builder_vis).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in settable_consts.clone() {
        writeln!(
            out,
            "            {ident}: {ty},",
            ident = InnerValueField(&field.ident.sym),
            ty = if field.value.is_some() {
                "None"
            } else {
                paths::UNSET
            },
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();

    // `ComponentBuilder::{with_*}`
    writeln!(
        out,
        "impl{gen} {comp}Builder{gen} {{",
        comp = comp_ident,
        gen = if non_optional_consts.clone().next().is_some() {
            Left(Angle(CommaSeparated(builder_ty_params.clone())))
        } else {
            Right("")
        }
    )
    .unwrap();

    for field in optional_consts.clone() {
        // They just assign a new value to `Option<T>`
        writeln!(
            out,
            "    {vis} fn {method}(self, {ident}: {ty}) -> Self {{",
            vis = field.vis.to_token_stream(),
            method = FactorySetterForField(&field.ident.sym),
            ident = field.ident.sym,
            ty = field.ty.to_token_stream(),
        )
        .unwrap();
        writeln!(
            out,
            "        Self {{ {ident}: {some}({ident}), ..self }}",
            some = paths::SOME,
            ident = field.ident.sym,
        )
        .unwrap();
        writeln!(out, "    }}",).unwrap();
    }

    for (i, field) in non_optional_consts.clone().enumerate() {
        // They each change one type parameter of `ComponentBuilder`
        let new_builder_ty = format!(
            "{comp}Builder<{gen}>",
            comp = comp_ident,
            gen = CommaSeparated(
                builder_ty_params
                    .clone()
                    .map(Left)
                    .replace_at(i, Right(field.ty.to_token_stream()))
            )
        );
        writeln!(
            out,
            "    {vis} fn {method}(self, {ident}: {ty}) -> {new_bldr_ty} {{",
            vis = field.vis.to_token_stream(),
            method = FactorySetterForField(&field.ident.sym),
            ident = field.ident.sym,
            ty = field.ty.to_token_stream(),
            new_bldr_ty = new_builder_ty,
        )
        .unwrap();
        writeln!(
            out,
            "        {comp}Builder {{ {fields} }}",
            comp = comp_ident,
            fields = CommaSeparated(settable_consts.clone().map(|field2| {
                if field2.ident.sym == field.ident.sym {
                    // Replace with the new value
                    format!(
                        "{}: {}",
                        InnerValueField(&field2.ident.sym),
                        field2.ident.sym
                    )
                } else {
                    // Use the old value
                    format!(
                        "{}: self.{}",
                        InnerValueField(&field2.ident.sym),
                        field2.ident.sym
                    )
                }
            }))
        )
        .unwrap();
        writeln!(out, "    }}",).unwrap();
    }
    writeln!(out, "}}").unwrap();

    // TODO: `Builder::build`

    // TODO: setters/getters/subscriptions

    out
}

// Lower-level codegen utils
// -------------------------------------------------------------------

macro_rules! fn_fmt_write {
    (|$this:ident| ($($fmt:tt)*)) => {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let $this = self;
            write!(f, $($fmt)*)
        }
    };
}

struct Angle<T>(T);
impl<T: fmt::Display> fmt::Display for Angle<T> {
    fn_fmt_write! { |this| ("<{}>", this.0) }
}

#[derive(Clone)]
struct FactoryGenParamNameForField<T>(T);
impl<T: fmt::Display> fmt::Display for FactoryGenParamNameForField<T> {
    fn_fmt_write! { |this| ("T_{}", this.0) }
}

struct FactorySetterForField<T>(T);
impl<T: fmt::Display> fmt::Display for FactorySetterForField<T> {
    fn_fmt_write! { |this| ("with_{}", this.0) }
}

struct InnerValueField<T>(T);
impl<T: fmt::Display> fmt::Display for InnerValueField<T> {
    fn_fmt_write! { |this| ("value_{}", this.0) }
}

struct EventInnerSubList<T>(T);
impl<T: fmt::Display> fmt::Display for EventInnerSubList<T> {
    fn_fmt_write! { |this| ("subscriptions_{}", this.0) }
}

struct EventDynHandlerTy<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventDynHandlerTy<'_> {
    fn_fmt_write! { |this| (
        "dyn {fn}({params})",
        fn = paths::FN,
        params = CommaSeparated(this.0.iter()
            .map(|arg| match arg {
                syn::FnArg::Receiver(_) => unreachable!(),
                syn::FnArg::Typed(pat) => pat.ty.to_token_stream(),
            }))
    ) }
}

struct EventBoxHandlerTy<'a>(&'a [syn::FnArg]);
impl fmt::Display for EventBoxHandlerTy<'_> {
    fn_fmt_write! { |this| (
        "{bx}<{inner}>",
        bx = paths::BOX,
        inner = EventDynHandlerTy(this.0)
    ) }
}

struct CommaSeparated<T>(T);
impl<T> fmt::Display for CommaSeparated<T>
where
    T: Clone + IntoIterator,
    T::Item: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut it = self.0.clone().into_iter();
        if let Some(e) = it.next() {
            write!(f, "{}", e)?;
            drop(e);
            for e in it {
                write!(f, ", {}", e)?;
            }
        }
        Ok(())
    }
}

/// Construct a mapping from names to values of type `T`. Reports an error if
/// duplicate names are detected.
fn make_name_map<T>(
    it: impl IntoIterator<Item = (T, sem::Ident)>,
    diag: &mut Diag,
) -> HashMap<String, T> {
    let mut multimap = HashMap::<String, Vec<_>>::new();

    for (val, ident) in it {
        multimap
            .entry(ident.sym)
            .or_default()
            .push((ident.span, val));
    }

    // Check duplicates
    for (ident_st, occurrences) in multimap.iter() {
        if occurrences.len() <= 1 {
            continue;
        }

        diag.emit(&[Diagnostic {
            level: Level::Error,
            message: format!("`{}` is defined for multiple times", ident_st),
            code: None,
            spans: occurrences
                .iter()
                .filter_map(|a| a.0)
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .into_iter()
                .collect(),
        }]);
    }

    // Convert to the desired hashmap type
    multimap
        .into_iter()
        .map(|(ident_st, mut occurrences)| (ident_st, occurrences.pop().unwrap().1))
        .collect()
}
