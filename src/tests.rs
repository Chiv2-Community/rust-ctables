use super::*;
use syn::parse::Parser;

fn rendered_docs(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            let syn::Meta::NameValue(v) = &attr.meta else {
                return None;
            };
            let syn::Expr::Lit(v) = &v.value else {
                return None;
            };
            let syn::Lit::Str(v) = &v.lit else {
                return None;
            };
            Some(v.value())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn entry_safety_requirements_share_one_section_with_native_contract() {
    let entries: Entries = syn::parse_quote! {
        /// Trim caches.
        ///
        /// # Safety
        /// Must run on the main thread.
        ///
        /// # Panics
        /// Native failures are not Rust panics.
        trim(this: *mut Self, caches: bool);
    };
    let Entry::Method(method) = &entries.0[0] else {
        panic!("expected method");
    };
    let generated: syn::ImplItemFn = syn::parse2(forwarding(method, &quote!(()))).unwrap();
    let docs = rendered_docs(&generated.attrs);
    assert_eq!(docs.matches("# Safety").count(), 1);
    assert!(docs.contains("Trim caches.\n\n"));
    let native = docs.find("The native object").unwrap();
    let specific = docs.find("Must run on the main thread.").unwrap();
    let other = docs.find("# Panics").unwrap();
    assert!(native < specific && specific < other);
}

#[test]
fn multiline_safety_docs_and_default_sections_are_supported() {
    let attrs: Vec<Attribute> =
        vec![syn::parse_quote!(#[doc = "Summary.\n\n# Safety\nMust own the allocation."])];
    let docs = rendered_docs(&safety_docs(&attrs));
    assert_eq!(docs.matches("# Safety").count(), 1);
    assert!(docs.contains("Must own the allocation."));
    assert!(docs.contains("The native object"));
    let default = rendered_docs(&safety_docs(&[]));
    assert_eq!(default.matches("# Safety").count(), 1);
    assert!(default.contains("The native object"));
}

#[test]
fn example_headings_are_not_mistaken_for_safety_sections() {
    let attrs =
        vec![syn::parse_quote!(#[doc = "Example:\n```text\n# Safety\n```\n\n    # Safety"])];
    let docs = rendered_docs(&safety_docs(&attrs));
    let generic = docs.find("The native object").unwrap();
    let last_heading = docs.rfind("# Safety").unwrap();
    assert!(generic > last_heading);
    assert!(docs.contains("```text\n# Safety\n```"));
}

fn reject(tokens: Tokens, expected: &str) {
    let item = syn::parse2(tokens).unwrap();
    let error = start(item).unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn rejects_unsupported_layout_declarations() {
    reject(
        quote!(
            struct Generic<T> {
                value: T,
            }
        ),
        "generic",
    );
    reject(
        quote!(
            struct Tuple(i32);
        ),
        "named fields",
    );
    reject(
        quote!(
            #[repr(C, packed)]
            struct Packed {
                value: i32,
            }
        ),
        "supplies repr(C)",
    );
}

#[test]
fn rejects_invalid_virtual_declarations() {
    reject(
        quote!(
            struct Public {
                #[vtable(get(&self);)]
                pub vtable: *const (),
            }
        ),
        "private",
    );
    reject(
        quote!(
            struct Wrong {
                #[vtable(get(&self);)]
                vtable: usize,
            }
        ),
        "*const ()",
    );
    reject(
        quote!(
            struct Zero {
                #[vtable(pad(0);)]
                vtable: *const (),
            }
        ),
        "positive",
    );
    reject(
        quote!(
            struct Bad {
                #[vtable(get(self);)]
                vtable: *const (),
            }
        ),
        "&self",
    );
    reject(
        quote!(
            struct Bad {
                #[vtable(get(this: &Self);)]
                vtable: *const (),
            }
        ),
        "raw receiver",
    );
    reject(
        quote!(
            struct Bad {
                #[vtable(get(&self); get(&self);)]
                vtable: *const (),
            }
        ),
        "conflicting",
    );
    reject(
        quote!(
            struct Bad {
                #[inherit]
                base: *const Other,
            }
        ),
        "concrete type",
    );
    reject(
        quote!(
            struct Bad {
                #[inherit]
                #[cfg(test)]
                base: Other,
            }
        ),
        "cannot be combined",
    );
    reject(
        quote!(
            struct Bad {
                #[vtable(#[cfg(test)] get(&self);)]
                vtable: *const (),
            }
        ),
        "only doc",
    );
}

#[test]
fn generates_c_layouts_private_tables_and_unsafe_inline_calls() {
    let generated = start(syn::parse_quote! {
        pub struct Native {
            #[vtable(pad(2); get(&self) -> i32;)]
            table: *const (),
            value: i32,
        }
    })
    .unwrap();
    let file: syn::File = syn::parse2(generated).unwrap();
    let table = file
        .items
        .iter()
        .find_map(|i| match i {
            syn::Item::Struct(s) if s.ident == "NativeTableVTable" => Some(s),
            _ => None,
        })
        .unwrap();
    assert!(matches!(table.vis, Visibility::Inherited));
    assert_eq!(table.fields.len(), 2);
    let method = file
        .items
        .iter()
        .find_map(|i| match i {
            syn::Item::Impl(i) => i.items.iter().find_map(|m| {
                if let syn::ImplItem::Fn(f) = m {
                    Some(f)
                } else {
                    None
                }
            }),
            _ => None,
        })
        .unwrap();
    assert!(method.sig.unsafety.is_some());
    assert!(method.attrs.iter().any(|a| a.path().is_ident("inline")));
}

#[test]
fn absolute_offsets_sort_methods_and_mix_with_relative_padding() {
    let method = |name: &str| LayoutEntry::Method(format_ident!("{name}"), name.parse().unwrap());
    let fields = layout_fields(
        &[
            LayoutEntry::Offset(0x20),
            method("later"),
            LayoutEntry::Offset(0),
            method("first"),
            LayoutEntry::Pad(1),
            method("middle"),
            method("next"),
        ],
        8,
    )
    .unwrap();
    assert_eq!(fields.len(), 5);
    let names = [0, 2, 3, 4].map(|i| syn::parse2::<Ident>(fields[i].clone()).unwrap().to_string());
    assert_eq!(names, ["first", "middle", "next", "later"]);
    let padding: syn::Field = syn::Field::parse_named.parse2(fields[1].clone()).unwrap();
    let Type::Array(array) = padding.ty else {
        panic!("expected slot padding")
    };
    let syn::Expr::Lit(length) = array.len else {
        panic!("expected padding length")
    };
    let syn::Lit::Int(length) = length.lit else {
        panic!("expected integer length")
    };
    assert_eq!(length.base10_parse::<usize>().unwrap(), 1);
}

#[test]
fn offsets_validate_target_alignment_overlap_and_overflow() {
    let method = || LayoutEntry::Method(format_ident!("value"), quote!(value));
    let overlap = layout_fields(&[method(), LayoutEntry::Offset(0), method()], 8).unwrap_err();
    assert!(overlap.to_string().contains("overlapping"));
    let misaligned = layout_fields(&[LayoutEntry::Offset(4), method()], 8).unwrap_err();
    assert!(misaligned.to_string().contains("aligned"));
    assert!(layout_fields(&[LayoutEntry::Offset(4), method()], 4).is_ok());
    assert!(layout_fields(&[LayoutEntry::Pad(usize::MAX)], 8).is_err());
    assert!(layout_fields(&[LayoutEntry::Offset(usize::MAX - 7), method()], 8).is_err());
}
