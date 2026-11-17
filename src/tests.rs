use super::*;
use syn::parse::Parser;

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
