//! Nightly raw self receivers dispatch through native objects without references.
#![cfg(feature = "nightly-receivers")]
#![feature(arbitrary_self_types_pointers)]
#![allow(dead_code, missing_docs, reason = "native ABI test declarations")]

use rust_ctables::cpp_struct;
use rust_ctables_native_tests::PointerRaw;

#[cpp_struct]
pub struct RawChild {
    #[inherit]
    base: rust_ctables_native_tests::PointerRaw,
}

#[cpp_struct]
pub struct Right {
    #[vtable(
        right_value(self: *const Self) -> i32;
        set_right(self: *mut Self, value: i32);
    )]
    vtable: *const (),
    y: i32,
    reserved: i32,
}

#[cpp_struct]
pub struct Left {
    #[vtable(left_value(self: *const Self) -> i32;)]
    vtable: *const (),
    x: i32,
    reserved: i32,
}

#[cpp_struct]
pub struct Multiple {
    #[inherit]
    left: Left,
    #[inherit]
    right: Right,
    extra: i32,
}

#[cpp_struct]
pub struct Grandchild {
    #[inherit]
    base: Multiple,
}

#[cfg(target_env = "msvc")]
#[cpp_struct]
pub struct PointerExtension {
    #[inherit]
    #[vtable(
        offset(0x20);
        grandchild_value(self: *const Self) -> i32;
    )]
    base: rust_ctables_native_tests::Extended,
}

#[cfg(not(target_env = "msvc"))]
#[cpp_struct]
pub struct PointerExtension {
    #[inherit]
    #[vtable(
        offset(0x28);
        grandchild_value(self: *const Self) -> i32;
    )]
    base: rust_ctables_native_tests::Extended,
}

unsafe extern "C" {
    fn fixture_raw() -> *mut PointerRaw;
    fn fixture_delete_raw(value: *mut PointerRaw);
    fn fixture_new() -> *mut Multiple;
    fn fixture_delete(value: *mut Multiple);
    fn fixture_grandchild() -> *mut Grandchild;
    fn fixture_delete_grandchild(value: *mut Grandchild);
    fn fixture_right_value(value: *mut Multiple) -> i32;
    fn fixture_extended_grandchild() -> *mut PointerExtension;
    fn fixture_delete_extended_grandchild(value: *mut PointerExtension);
}

#[test]
fn pointer_methods_preserve_constness_and_inherited_dispatch() {
    unsafe {
        let ptr = fixture_raw();
        assert_eq!(ptr.cast_const().get(), 7);
        assert_eq!(ptr.add_amount(5), 12);
        let child = ptr.cast::<RawChild>();
        assert_eq!(child.add_amount(8), 20);
        assert_eq!(child.cast_const().get(), 20);
        fixture_delete_raw(ptr);
    }
}

#[test]
fn pointer_methods_adjust_secondary_bases_and_forward_transitively() {
    unsafe {
        let ptr = fixture_new();
        assert_eq!(ptr.cast_const().right_value(), 23);
        ptr.set_right(50);
        assert_eq!(fixture_right_value(ptr), 53);
        assert_eq!(ptr.cast_const().right_value(), 53);
        fixture_delete(ptr);

        let ptr = fixture_grandchild();
        assert_eq!(ptr.cast_const().left_value(), 113);
        ptr.set_right(30);
        assert_eq!(ptr.cast_const().right_value(), 33);
        fixture_delete_grandchild(ptr);
    }
}

#[test]
fn extensions_support_pointer_self_receivers() {
    unsafe {
        let ptr = fixture_extended_grandchild();
        assert_eq!(ptr.cast_const().grandchild_value(), 33);
        assert_eq!(PointerExtension::right_extra(ptr, 7), 30);
        assert_eq!(ptr.cast_const().grandchild_value(), 40);
        fixture_delete_extended_grandchild(ptr);
    }
}
