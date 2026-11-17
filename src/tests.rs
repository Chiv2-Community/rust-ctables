use super::*;

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
