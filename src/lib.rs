#![doc = include_str!("../README.md")]
#![doc = include_str!("../tests/compile-fail.md")]
#![forbid(unsafe_code)]
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use std::hash::{Hash, Hasher};
use syn::visit_mut::VisitMut;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Error, Fields, FnArg, Ident, ItemStruct, Pat, ReturnType, Token, Type, TypePath,
    Visibility,
};

mod primary;

#[derive(Clone)]
struct Method {
    attrs: Vec<Attribute>,
    name: Ident,
    kind: Ident,
    args: Vec<(Ident, Type)>,
    output: ReturnType,
}

enum Entry {
    Pad(usize),
    Offset(usize),
    Method(Method),
}

struct Entries(Vec<Entry>);
impl Parse for Entries {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            let attrs = input.call(Attribute::parse_outer)?;
            let name: Ident = input.parse()?;
            let content;
            syn::parenthesized!(content in input);
            if name == "pad" || name == "offset" {
                if !attrs.is_empty() {
                    return Err(Error::new(
                        name.span(),
                        "layout directives cannot have attributes",
                    ));
                }
                let count: syn::LitInt = content.parse()?;
                let count = count.base10_parse::<usize>()?;
                if count == 0 && name == "pad" {
                    return Err(Error::new(name.span(), "pad count must be positive"));
                }
                if !content.is_empty() {
                    return Err(content.error("expected one integer literal"));
                }
                entries.push(if name == "pad" {
                    Entry::Pad(count)
                } else {
                    Entry::Offset(count)
                });
            } else {
                let mut args =
                    Punctuated::<FnArg, Token![,]>::parse_terminated(&content)?.into_iter();
                let receiver = args.next().ok_or_else(|| {
                    Error::new(
                        name.span(),
                        "expected &self, &mut self, self: *const/*mut Self, or this: *const/*mut Self",
                    )
                })?;
                let kind = receiver_kind(&receiver)?;
                let mut named = Vec::new();
                for arg in args {
                    let FnArg::Typed(arg) = arg else {
                        return Err(Error::new(arg.span(), "unexpected receiver"));
                    };
                    let Pat::Ident(pat) = *arg.pat else {
                        return Err(Error::new(arg.span(), "arguments must have names"));
                    };
                    if pat.by_ref.is_some() || pat.subpat.is_some() || pat.ident == "this" {
                        return Err(Error::new(
                            pat.span(),
                            "expected a plain argument name other than this",
                        ));
                    }
                    named.push((pat.ident, *arg.ty));
                }
                entries.push(Entry::Method(Method {
                    attrs,
                    name,
                    kind: Ident::new(kind, proc_macro2::Span::call_site()),
                    args: named,
                    output: input.parse()?,
                }));
            }
            input.parse::<Token![;]>()?;
        }
        Ok(Self(entries))
    }
}

fn receiver_kind(arg: &FnArg) -> syn::Result<&'static str> {
    let (ty, is_self) = match arg {
        FnArg::Receiver(r) if r.reference.is_some() && r.colon_token.is_none() => {
            return Ok(if r.mutability.is_some() {
                "mutable"
            } else {
                "shared"
            });
        }
        FnArg::Receiver(r) if r.colon_token.is_some() && r.mutability.is_none() => (&*r.ty, true),
        FnArg::Typed(arg) if matches!(&*arg.pat, Pat::Ident(p) if p.ident == "this" && p.by_ref.is_none() && p.mutability.is_none() && p.subpat.is_none()) => {
            (&*arg.ty, false)
        }
        _ => {
            return Err(Error::new(
                arg.span(),
                "expected &self, &mut self, or a raw receiver self/this: *const/*mut Self",
            ))
        }
    };
    match ty {
        Type::Ptr(p) if matches!(&*p.elem, Type::Path(t) if t.path.is_ident("Self")) => {
            Ok(match (is_self, p.mutability.is_some()) {
                (true, true) => "pointer_mut",
                (true, false) => "pointer_const",
                (false, true) => "raw_mut",
                (false, false) => "raw_const",
            })
        }
        _ => Err(Error::new(
            arg.span(),
            "raw receiver must be self/this: *const Self or self/this: *mut Self",
        )),
    }
}

// Metadata passed between expansions is tokenized, never kept in process-global state.
impl Parse for Method {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let kind = input.parse()?;
        let name = input.parse()?;
        let content;
        syn::parenthesized!(content in input);
        let args = Punctuated::<syn::PatType, Token![,]>::parse_terminated(&content)?;
        let mut named = Vec::new();
        for arg in args {
            let Pat::Ident(p) = *arg.pat else {
                return Err(input.error("invalid metadata argument"));
            };
            named.push((p.ident, *arg.ty));
        }
        let output = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self {
            attrs,
            name,
            kind,
            args: named,
            output,
        })
    }
}

fn method_tokens(m: &Method) -> Tokens {
    let Method {
        attrs,
        name,
        kind,
        args,
        output,
    } = m;
    let args = args.iter().map(|(n, t)| quote!(#n: #t));
    quote!(#(#attrs)* #kind #name(#(#args),*) #output;)
}

#[proc_macro_attribute]
/// Generate native C++ layout and virtual/inherited forwarding methods.
pub fn cpp_struct(args: TokenStream, item: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return Error::new(
            proc_macro2::Span::call_site(),
            "cpp_struct takes no arguments",
        )
        .into_compile_error()
        .into();
    }
    let item = parse_macro_input!(item as ItemStruct);
    start(item).unwrap_or_else(Error::into_compile_error).into()
}

fn start(mut item: ItemStruct) -> syn::Result<Tokens> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(
            item.generics.span(),
            "generic C++ layouts are not supported",
        ));
    }
    for attr in &item.attrs {
        if attr.path().is_ident("repr") {
            return Err(Error::new(
                attr.span(),
                "cpp_struct supplies repr(C); remove explicit repr",
            ));
        }
    }
    let Fields::Named(fields) = &mut item.fields else {
        return Err(Error::new(item.span(), "expected named fields"));
    };
    let mut parents = Vec::new();
    for field in &mut fields.named {
        let inherit = field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("inherit"))
            .collect::<Vec<_>>();
        if inherit.len() > 1 {
            return Err(Error::new(field.span(), "duplicate inherit attribute"));
        }
        if let Some(attr) = inherit.first() {
            if !matches!(attr.meta, syn::Meta::Path(_)) {
                return Err(Error::new(attr.span(), "inherit takes no arguments"));
            }
            let Type::Path(ty) = &field.ty else {
                return Err(Error::new(
                    field.ty.span(),
                    "inherit requires a concrete type path",
                ));
            };
            if ty.qself.is_some() || ty.path.segments.iter().any(|s| !s.arguments.is_empty()) {
                return Err(Error::new(
                    ty.span(),
                    "inherit requires a non-generic type path",
                ));
            }
            if field.attrs.iter().any(|a| a.path().is_ident("cfg")) {
                return Err(Error::new(
                    field.span(),
                    "inherit cannot be combined with cfg",
                ));
            }
            parents.push((field.ident.clone().unwrap(), ty.clone()));
        }
        field.attrs.retain(|a| !a.path().is_ident("inherit"));
    }
    advance(item, parents, Vec::new())
}

// Each parent exports a metadata macro. Ask it for signatures, then continue here.
fn advance(
    item: ItemStruct,
    mut parents: Vec<(Ident, TypePath)>,
    inherited: Vec<(Ident, Vec<Method>)>,
) -> syn::Result<Tokens> {
    if parents.is_empty() {
        return finish(item, inherited);
    }
    let (field, ty) = parents.remove(0);
    let mut metadata = ty.path.clone();
    let last = metadata.segments.last_mut().unwrap();
    last.ident = format_ident!("__ctables_{}", last.ident);
    let mut signature = ty.path.clone();
    let last = signature.segments.last_mut().unwrap();
    last.ident = format_ident!("__Ctables{}", last.ident);
    let remaining = parents.iter().map(|(f, t)| quote!(#f: #t;));
    let collected = inherited.iter().map(|(f, ms)| {
        let ms = ms.iter().map(method_tokens);
        quote!(#f { #(#ms)* })
    });
    Ok(quote! {
        #metadata! { (#ty) (#signature) (::rust_ctables::__cpp_inherit)
            { { #item } { #(#remaining)* } { #(#collected)* } #field }
        }
    })
}

struct Continuation {
    item: ItemStruct,
    parents: Vec<(Ident, TypePath)>,
    inherited: Vec<(Ident, Vec<Method>)>,
}
impl Parse for Continuation {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let state;
        syn::braced!(state in input);
        let body;
        syn::braced!(body in state);
        let item = body.parse()?;
        let remaining;
        syn::braced!(remaining in state);
        let mut parents = Vec::new();
        while !remaining.is_empty() {
            let f = remaining.parse()?;
            remaining.parse::<Token![:]>()?;
            let t = remaining.parse()?;
            remaining.parse::<Token![;]>()?;
            parents.push((f, t));
        }
        let collected;
        syn::braced!(collected in state);
        let mut inherited = Vec::new();
        while !collected.is_empty() {
            let f = collected.parse()?;
            let methods;
            syn::braced!(methods in collected);
            let mut ms = Vec::new();
            while !methods.is_empty() {
                ms.push(methods.parse()?);
            }
            inherited.push((f, ms));
        }
        let field = state.parse()?;
        let methods;
        syn::braced!(methods in input);
        let mut ms = Vec::new();
        while !methods.is_empty() {
            ms.push(methods.parse()?);
        }
        inherited.push((field, ms));
        Ok(Self {
            item,
            parents,
            inherited,
        })
    }
}

#[doc(hidden)]
#[proc_macro]
pub fn __cpp_inherit(input: TokenStream) -> TokenStream {
    let Continuation {
        item,
        parents,
        inherited,
    } = parse_macro_input!(input as Continuation);
    advance(item, parents, inherited)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn receiver(kind: &Ident) -> (Tokens, Tokens, Tokens) {
    match kind.to_string().as_str() {
        "shared" => (
            quote!(&self),
            quote!(*const Self),
            quote!(self as *const Self),
        ),
        "mutable" => (
            quote!(&mut self),
            quote!(*mut Self),
            quote!(self as *mut Self),
        ),
        "raw_const" => (quote!(this: *const Self), quote!(*const Self), quote!(this)),
        "raw_mut" => (quote!(this: *mut Self), quote!(*mut Self), quote!(this)),
        "pointer_const" => (quote!(self: *const Self), quote!(*const Self), quote!(self)),
        "pointer_mut" => (quote!(self: *mut Self), quote!(*mut Self), quote!(self)),
        _ => unreachable!(),
    }
}

fn forwarding(m: &Method, call: &Tokens) -> Tokens {
    let Method {
        attrs,
        name,
        kind,
        args,
        output,
    } = m;
    let attrs = safety_docs(attrs);
    let (recv, _, _) = receiver(kind);
    let args = args.iter().map(|(n, t)| quote!(#n: #t));
    quote! {
        #(#attrs)*
        #[inline(always)]
        pub unsafe fn #name(#recv, #(#args),*) #output { unsafe { #call } }
    }
}

fn safety_docs(attrs: &[Attribute]) -> Vec<Attribute> {
    let mut docs = Vec::new();
    let mut inserted = false;
    let mut fence = None;
    for attr in attrs {
        let syn::Meta::NameValue(value) = &attr.meta else {
            docs.push(attr.clone());
            continue;
        };
        let syn::Expr::Lit(value) = &value.value else {
            docs.push(attr.clone());
            continue;
        };
        let syn::Lit::Str(value) = &value.lit else {
            docs.push(attr.clone());
            continue;
        };
        for line in value.value().split('\n') {
            docs.push(syn::parse_quote!(#[doc = #line]));
            let trimmed = line.trim_start();
            // Ignore headings inside indented or fenced code examples.
            if line.len() - trimmed.len() >= 4 {
                continue;
            }
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let marker = trimmed.as_bytes()[0];
                let count = trimmed.bytes().take_while(|&b| b == marker).count();
                match fence {
                    Some((open, length)) if open == marker && count >= length => fence = None,
                    None => fence = Some((marker, count)),
                    _ => {}
                }
                continue;
            }
            if fence.is_none() && trimmed.trim_end_matches('#').trim() == "# Safety" && !inserted {
                append_native_safety(&mut docs);
                inserted = true;
            }
        }
    }
    if !inserted {
        docs.push(syn::parse_quote!(#[doc = ""]));
        docs.push(syn::parse_quote!(#[doc = "# Safety"]));
        append_native_safety(&mut docs);
    }
    docs
}

fn append_native_safety(docs: &mut Vec<Attribute>) {
    for line in [
        "",
        "The native object and vtable must match this declaration's layout, slots and ABI",
        "and remain valid for native accesses. All native argument, ownership, aliasing",
        "and thread rules apply.",
        "",
    ] {
        docs.push(syn::parse_quote!(#[doc = #line]));
    }
}

// These phases mirror the emitted layout, forwarding impls and metadata.
#[expect(
    clippy::too_many_lines,
    reason = "single ordered expansion pass over declared fields"
)]
fn finish(mut item: ItemStruct, inherited: Vec<(Ident, Vec<Method>)>) -> syn::Result<Tokens> {
    let name = item.ident.clone();
    let Fields::Named(fields) = &mut item.fields else {
        unreachable!()
    };
    let mut tables = Vec::new();
    let mut impls = Vec::new();
    let mut methods = Vec::new();
    let mut names = [
        "__ctables_primary_vptr",
        "__ctables_primary_has_slot",
        "__CTABLES_HAS_PRIMARY_VTABLE",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<std::collections::HashSet<_>>();
    let parent_fields = inherited
        .iter()
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    let mut table_layouts = Vec::new();
    for (field, ms) in inherited {
        let parent_ty = &fields
            .named
            .iter()
            .find(|f| f.ident.as_ref() == Some(&field))
            .unwrap()
            .ty;
        for m in ms {
            if !names.insert(m.name.to_string()) {
                return Err(Error::new(
                    m.name.span(),
                    "conflicting inherited method; use the parent subobjects directly",
                ));
            }
            let method = &m.name;
            let args = m.args.iter().map(|(n, _)| n);
            let (_, _, this) = receiver(&m.kind);
            let call = match m.kind.to_string().as_str() {
                "raw_const" | "pointer_const" => {
                    quote!(<#parent_ty>::#method(::core::ptr::addr_of!((*#this).#field), #(#args),*))
                }
                "raw_mut" | "pointer_mut" => {
                    quote!(<#parent_ty>::#method(::core::ptr::addr_of_mut!((*#this).#field), #(#args),*))
                }
                _ => quote!(self.#field.#method(#(#args),*)),
            };
            impls.push(forwarding(&m, &call));
            methods.push(m);
        }
    }
    for field in &mut fields.named {
        let attrs = field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("vtable"))
            .collect::<Vec<_>>();
        if attrs.is_empty() {
            continue;
        }
        if attrs.len() != 1 {
            return Err(Error::new(field.span(), "duplicate vtable attribute"));
        }
        let extends = parent_fields.contains(field.ident.as_ref().unwrap());
        if !extends && !matches!(field.vis, Visibility::Inherited) {
            return Err(Error::new(
                field.vis.span(),
                "vtable pointers must be private",
            ));
        }
        if field.attrs.iter().any(|a| a.path().is_ident("cfg")) {
            return Err(Error::new(
                field.span(),
                "conditional vtable fields are not supported",
            ));
        }
        if !extends
            && !matches!(&field.ty, Type::Ptr(p) if p.const_token.is_some() && matches!(&*p.elem, Type::Tuple(t) if t.elems.is_empty()))
        {
            return Err(Error::new(
                field.ty.span(),
                "declare the vtable field as *const ()",
            ));
        }
        let entries = attrs[0].parse_args::<Entries>()?;
        if extends && !matches!(entries.0.first(), Some(Entry::Offset(_))) {
            return Err(Error::new(
                field.span(),
                "inherited vtable extensions must start with an explicit offset",
            ));
        }
        let fname = field.ident.as_ref().unwrap();
        let table = if fname == "vtable" {
            format_ident!("{}VTable", name)
        } else {
            format_ident!(
                "{}{}VTable",
                name,
                fname
                    .to_string()
                    .split('_')
                    .map(|s| {
                        let mut c = s.chars();
                        c.next()
                            .map(|x| x.to_uppercase().collect::<String>() + c.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<String>()
            )
        };
        let mut layout = Vec::new();
        for entry in entries.0 {
            match entry {
                Entry::Pad(n) => {
                    layout.push(LayoutEntry::Pad(n));
                }
                Entry::Offset(n) => layout.push(LayoutEntry::Offset(n)),
                Entry::Method(mut m) => {
                    if m.attrs.iter().any(|a| !a.path().is_ident("doc")) {
                        return Err(Error::new(
                            m.name.span(),
                            "only doc attributes are supported on methods",
                        ));
                    }
                    if !names.insert(m.name.to_string()) {
                        return Err(Error::new(
                            m.name.span(),
                            "conflicting vtable or inherited method name",
                        ));
                    }
                    // Self in argument/return positions refers to the native type, not the vtable.
                    let mut replace = Replace(&name);
                    for (_, t) in &mut m.args {
                        replace.visit_type_mut(t);
                    }
                    if let ReturnType::Type(_, t) = &mut m.output {
                        replace.visit_type_mut(t);
                    }
                    let method = &m.name;
                    let (_, this_ty, this) = receiver(&m.kind);
                    let this_ty: Type = syn::parse2(this_ty)?;
                    let mut this_ty = this_ty;
                    replace.visit_type_mut(&mut this_ty);
                    let types = m.args.iter().map(|(_, t)| t);
                    let output = &m.output;
                    layout.push(LayoutEntry::Method(
                        method.clone(),
                        quote!(#method: unsafe extern "C" fn(#this_ty, #(#types),*) #output),
                    ));
                    let args = m.args.iter().map(|(n, _)| n);
                    let owner = Ident::new("__owner", proc_macro2::Span::mixed_site());
                    let native = Ident::new("__native", proc_macro2::Span::mixed_site());
                    let parent_ty = &field.ty;
                    let vptr = if extends {
                        quote!(<#parent_ty>::__ctables_primary_vptr(::core::ptr::addr_of!((*#owner).#fname)))
                    } else {
                        quote!(::core::ptr::addr_of!((*#owner).#fname))
                    };
                    let adjusted = if matches!(
                        m.kind.to_string().as_str(),
                        "mutable" | "raw_mut" | "pointer_mut"
                    ) {
                        quote!(#native.cast_mut().cast::<Self>())
                    } else {
                        quote!(#native.cast::<Self>())
                    };
                    impls.push(forwarding(
                        &m,
                        &quote!({
                            let #owner = #this;
                            let #native = #vptr;
                            ((*(*#native).cast::<#table>()).#method)(#adjusted, #(#args),*)
                        }),
                    ));
                    methods.push(m);
                }
            }
        }
        // The proc macro runs on the host, but offsets belong to the target ABI.
        for width in [16, 32, 64] {
            let cfg = width.to_string();
            let definition = match layout_fields(&layout, width / 8) {
                Ok(slots) => quote!(#[derive(Debug)] #[repr(C)] struct #table { #(#slots,)* }),
                Err(e) => e.into_compile_error(),
            };
            tables.push(quote!(#[cfg(target_pointer_width = #cfg)] #definition));
            if extends {
                tables.push(primary::validate_extension(&field.ty, &layout, width));
            }
        }
        table_layouts.push((fname.clone(), layout));
        if !extends {
            field.ty = syn::parse_quote!(*const #table);
        }
        field.attrs.retain(|a| !a.path().is_ident("vtable"));
    }
    impls.push(primary::api(fields, &parent_fields, &table_layouts));
    let metadata = metadata(&item, &methods);
    Ok(quote! {
        #[repr(C)] #item
        #(#tables)*
        impl #name { #(#impls)* }
        #metadata
    })
}

enum LayoutEntry {
    Pad(usize),
    Offset(usize),
    Method(Ident, Tokens),
}

fn layout_fields(entries: &[LayoutEntry], pointer_size: usize) -> syn::Result<Vec<Tokens>> {
    let mut cursor = 0usize;
    let mut extent = 0;
    let mut methods = Vec::new();
    let span = proc_macro2::Span::call_site();
    for entry in entries {
        match entry {
            LayoutEntry::Pad(n) => {
                cursor = n
                    .checked_mul(pointer_size)
                    .and_then(|n| cursor.checked_add(n))
                    .ok_or_else(|| {
                        Error::new(span, "vtable padding overflows the address range")
                    })?;
            }
            LayoutEntry::Offset(n) => {
                if n % pointer_size != 0 {
                    return Err(Error::new(
                        span,
                        format!("vtable offset {n:#x} must be aligned to {pointer_size} bytes"),
                    ));
                }
                cursor = *n;
            }
            LayoutEntry::Method(name, field) => {
                methods.push((cursor, name, field));
                cursor = cursor.checked_add(pointer_size).ok_or_else(|| {
                    Error::new(
                        name.span(),
                        "vtable method offset overflows the address range",
                    )
                })?;
            }
        }
        extent = extent.max(cursor);
    }
    methods.sort_by_key(|(offset, _, _)| *offset);
    let mut fields = Vec::new();
    let mut end = 0;
    for (offset, name, field) in methods {
        if offset < end {
            return Err(Error::new(
                name.span(),
                "overlapping vtable methods at the same offset",
            ));
        }
        append_padding(&mut fields, (offset - end) / pointer_size);
        fields.push(field.clone());
        end = offset + pointer_size;
    }
    append_padding(&mut fields, (extent - end) / pointer_size);
    Ok(fields)
}

fn append_padding(fields: &mut Vec<Tokens>, slots: usize) {
    if slots != 0 {
        let name = format_ident!("_pad_{}", fields.len());
        fields.push(quote!(#name: [usize; #slots]));
    }
}

struct Replace<'a>(&'a Ident);
impl VisitMut for Replace<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if matches!(ty, Type::Path(t) if t.path.is_ident("Self")) {
            let n = self.0;
            *ty = syn::parse_quote!(#n);
        } else {
            syn::visit_mut::visit_type_mut(self, ty);
        }
    }
}

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn metadata(item: &ItemStruct, methods: &[Method]) -> Tokens {
    let name = &item.ident;
    // Associated signature types keep inherited argument names resolved in the
    // parent's module, even when the child is in another module or crate.
    let signature = format_ident!("__Ctables{}", name);
    let metadata = format_ident!("__ctables_{}", name);
    let mut aliases = Vec::new();
    let mut definitions = Vec::new();
    let mut descriptors = Vec::new();
    for (i, m) in methods.iter().enumerate() {
        let mut args = Vec::new();
        for (j, (arg, ty)) in m.args.iter().enumerate() {
            let alias = format_ident!("M{}Arg{}", i, j);
            aliases.push(quote!(type #alias;));
            definitions.push(quote!(type #alias = #ty;));
            args.push(quote!(#arg: <$owner as $signature>::#alias));
        }
        let alias = format_ident!("M{}Return", i);
        let ret: Type = match &m.output {
            ReturnType::Default => syn::parse_quote!(()),
            ReturnType::Type(_, t) => *t.clone(),
        };
        aliases.push(quote!(type #alias;));
        definitions.push(quote!(type #alias = #ret;));
        let Method {
            attrs, kind, name, ..
        } = m;
        descriptors
            .push(quote!(#(#attrs)* #kind #name(#(#args),*) -> <$owner as $signature>::#alias;));
    }
    // Export a unique implementation name and re-export a predictable module-local
    // name. Distinct modules may declare types with the same short name.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    quote!(#item #(#definitions)*).to_string().hash(&mut hash);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let exported = format_ident!("__rust_ctables_{}_{:x}_{}", name, hash.finish(), unique);
    let vis = &item.vis;
    quote! {
        #[doc(hidden)] #[allow(non_camel_case_types)] #vis trait #signature { #(#aliases)* }
        impl #signature for #name { #(#definitions)* }
        #[doc(hidden)]
        #[macro_export]
        macro_rules! #exported {
            (($owner:ty) ($signature:path) ($callback:path) { $($state:tt)* }) => {
                $callback! { { $($state)* } { #(#descriptors)* } }
            };
        }
        #[doc(hidden)] #[allow(unused_imports)] #vis use #exported as #metadata;
    }
}

#[cfg(test)]
mod tests;
