use super::{layout_fields, Ident, LayoutEntry, Tokens, Type};
use quote::quote;

// Explicit offsets let extensions bind known native slots without assuming that
// a partial parent declaration describes its complete table.
fn offsets(layout: &[LayoutEntry], width: usize) -> syn::Result<Vec<usize>> {
    layout_fields(layout, width / 8)?;
    let mut cursor = 0usize;
    let mut offsets = Vec::new();
    for entry in layout {
        match entry {
            LayoutEntry::Pad(n) => cursor += n * (width / 8),
            LayoutEntry::Offset(n) => cursor = *n,
            LayoutEntry::Method(_, _) => {
                offsets.push(cursor);
                cursor += width / 8;
            }
        }
    }
    Ok(offsets)
}

pub fn validate_extension(parent: &Type, layout: &[LayoutEntry], width: usize) -> Tokens {
    let cfg = width.to_string();
    let Ok(offsets) = offsets(layout, width) else {
        // The table definition already emits a target-gated layout error.
        return Tokens::new();
    };
    quote! {
        #[cfg(target_pointer_width = #cfg)]
        const _: () = {
            assert!(<#parent>::__CTABLES_HAS_PRIMARY_VTABLE,
                "cannot extend a parent without a vtable");
            #(assert!(!<#parent>::__ctables_primary_has_slot(#offsets),
                "extension overlaps a bound inherited vtable slot");)*
        };
    }
}

pub fn api(
    fields: &syn::FieldsNamed,
    parents: &[Ident],
    layouts: &[(Ident, Vec<LayoutEntry>)],
) -> Tokens {
    let mut apis = Vec::new();
    for width in [16, 32, 64] {
        let cfg = width.to_string();
        let mut has_table = quote!(false);
        let mut has_slot = quote!(false);
        let mut vptr = quote!(panic!("native type has no primary vtable"));
        for field in fields.named.iter().rev() {
            let name = field.ident.as_ref().unwrap();
            let layout = layouts.iter().find(|(f, _)| f == name);
            let slots = layout
                .and_then(|(_, layout)| offsets(layout, width).ok())
                .unwrap_or_default();
            let bound = quote!(false #(|| offset == #slots)*);
            if parents.contains(name) {
                let ty = &field.ty;
                has_slot = quote!(if <#ty>::__CTABLES_HAS_PRIMARY_VTABLE {
                    #bound || <#ty>::__ctables_primary_has_slot(offset)
                } else { #has_slot });
                vptr = quote!(if <#ty>::__CTABLES_HAS_PRIMARY_VTABLE {
                    <#ty>::__ctables_primary_vptr(::core::ptr::addr_of!((*this).#name))
                } else { #vptr });
                has_table = quote!(<#ty>::__CTABLES_HAS_PRIMARY_VTABLE || #has_table);
            } else if layout.is_some() {
                has_table = quote!(true);
                has_slot = bound;
                vptr = quote!(::core::ptr::addr_of!((*this).#name).cast());
            }
        }
        apis.push(quote! {
            #[cfg(target_pointer_width = #cfg)]
            #[doc(hidden)]
            pub const __CTABLES_HAS_PRIMARY_VTABLE: bool = #has_table;

            #[cfg(target_pointer_width = #cfg)]
            #[doc(hidden)]
            pub const fn __ctables_primary_has_slot(offset: usize) -> bool {
                let _ = offset;
                #has_slot
            }

            #[cfg(target_pointer_width = #cfg)]
            /// # Safety
            /// `this` must address a live native object with a primary vtable.
            #[doc(hidden)]
            #[inline(always)]
            #[allow(unused_unsafe, reason = "types without vtables emit only a panic")]
            pub unsafe fn __ctables_primary_vptr(this: *const Self) -> *const *const () {
                let _ = this;
                unsafe { #vptr }
            }
        });
    }
    quote!(#(#apis)*)
}
