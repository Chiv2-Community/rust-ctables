//! Native ABI fixtures; public declarations are consumed by cross-crate tests.
#![cfg_attr(feature = "nightly-receivers", feature(arbitrary_self_types_pointers))]
#![allow(
    dead_code,
    missing_docs,
    reason = "native layout declarations used only by ABI tests"
)]
use rust_ctables::cpp_struct;

mod parents {
    use super::cpp_struct;

    #[repr(C)]
    pub struct Payload {
        pub amount: i32,
    }

    #[cpp_struct]
    pub struct Left {
        #[vtable(
            offset(0x8);
            untouched(&self) -> i32;
            apply(&self, payload: *const Payload) -> i32;
            offset(0);
            left_value(&self) -> i32;
        )]
        vtable: *const (),
        pub x: i32,
        reserved: i32,
    }

    #[cpp_struct]
    pub struct Right {
        #[vtable(
            offset(0x8);
            /// # Safety
            /// Caller must have exclusive access to this native subobject.
            set_right(&mut self, value: i32);
            offset(0);
            right_value(&self) -> i32;
        )]
        vtable: *const (),
        pub y: i32,
        reserved: i32,
    }
}

pub use parents::Payload;

#[cpp_struct]
pub struct DirectTables {
    #[vtable(left_value(&self) -> i32;)]
    primary: *const (),
    x: i32,
    reserved_left: i32,
    #[vtable(right_value(&self) -> i32;)]
    secondary: *const (),
    y: i32,
    reserved_right: i32,
    extra: i32,
}

#[cpp_struct]
pub struct Derived {
    #[inherit]
    pub left: parents::Left,
    #[inherit]
    pub right: parents::Right,
    pub extra: i32,
}

#[cpp_struct]
pub struct Grandchild {
    #[inherit]
    pub base: Derived,
}

#[cpp_struct]
pub struct Raw {
    #[vtable(
        pad(1);
        add(this: *mut Self, __owner: i32) -> i32;
        get(this: *const Self) -> i32;
    )]
    vtable: *const (),
    value: i32,
}

#[cpp_struct]
pub struct RawChild {
    #[inherit]
    pub base: Raw,
}

#[cfg(feature = "nightly-receivers")]
#[cpp_struct]
pub struct PointerRaw {
    #[vtable(
        pad(1);
        add_amount(self: *mut Self, amount: i32) -> i32;
        get(self: *const Self) -> i32;
    )]
    vtable: *const (),
    value: i32,
}

#[cpp_struct]
pub struct ExtendedLeft {
    #[inherit]
    #[vtable(
        offset(0x18);
        left_extra(this: *const Self, amount: i32) -> i32;
    )]
    base: parents::Left,
}

#[cpp_struct]
pub struct ExtendedRight {
    #[inherit]
    #[vtable(
        offset(0x10);
        right_extra(this: *mut Self, amount: i32) -> i32;
    )]
    base: parents::Right,
}

#[cpp_struct]
pub struct Extended {
    #[inherit]
    pub left: ExtendedLeft,
    #[inherit]
    pub right: ExtendedRight,
    extra: i32,
}

#[cpp_struct]
pub struct ExtendedDirect {
    #[inherit]
    #[vtable(
        offset(0x18);
        left_added(&self, amount: i32) -> i32;
    )]
    left: parents::Left,
    #[inherit]
    #[vtable(
        offset(0x10);
        right_added(&mut self, amount: i32) -> i32;
    )]
    right: parents::Right,
    extra: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn fixture_new() -> *mut Derived;
        fn fixture_delete(value: *mut Derived);
        fn fixture_grandchild() -> *mut Grandchild;
        fn fixture_delete_grandchild(value: *mut Grandchild);
        fn fixture_raw() -> *mut Raw;
        fn fixture_delete_raw(value: *mut Raw);
        fn fixture_derived_size() -> usize;
        fn fixture_derived_align() -> usize;
        fn fixture_right_offset(value: *mut Derived) -> usize;
        fn fixture_right_value(value: *mut Derived) -> i32;
        fn fixture_left_value(value: *mut Derived) -> i32;
        fn fixture_extended_left() -> *mut ExtendedLeft;
        fn fixture_delete_extended_left(value: *mut ExtendedLeft);
        fn fixture_extended() -> *mut Extended;
        fn fixture_delete_extended(value: *mut Extended);
        fn fixture_extended_size() -> usize;
        fn fixture_extended_left_size() -> usize;
        fn fixture_extended_right_offset(value: *mut Extended) -> usize;
        fn fixture_extended_right_value(value: *mut Extended) -> i32;
    }

    #[test]
    fn layout_matches_cpp_compiler() {
        unsafe {
            let ptr = fixture_new();
            assert_eq!(size_of::<Derived>(), fixture_derived_size());
            assert_eq!(align_of::<Derived>(), fixture_derived_align());
            assert_eq!(
                std::mem::offset_of!(Derived, right),
                fixture_right_offset(ptr)
            );
            fixture_delete(ptr);
        }
    }

    #[test]
    fn native_overrides_and_secondary_this_adjustment() {
        unsafe {
            let ptr = fixture_new();
            {
                let value = &mut *ptr;
                assert_eq!(value.left_value(), 13);
                assert_eq!(value.right_value(), 23);
                assert_eq!(value.untouched(), 20);
                value.set_right(50);
                assert_eq!(value.right_value(), 53);
            }
            assert_eq!(fixture_right_value(ptr), 53);
            assert_eq!(fixture_left_value(ptr), 13);
            fixture_delete(ptr);
        }
    }

    #[test]
    fn transitive_inheritance_preserves_native_overrides() {
        unsafe {
            let ptr = fixture_grandchild();
            {
                let value = &mut *ptr;
                assert_eq!(value.left_value(), 113);
                assert_eq!(value.right_value(), 23);
                value.set_right(30);
                assert_eq!(value.right_value(), 33);
            }
            fixture_delete_grandchild(ptr);
        }
    }

    #[test]
    fn raw_receivers_and_padding_dispatch_without_rust_references() {
        unsafe {
            let ptr = fixture_raw();
            assert_eq!(Raw::get(ptr), 7);
            assert_eq!(Raw::add(ptr, 5), 12);
            assert_eq!(Raw::get(ptr), 12);
            assert_eq!(RawChild::add(ptr.cast(), 8), 20);
            assert_eq!(RawChild::get(ptr.cast()), 20);
            fixture_delete_raw(ptr);
        }
    }

    #[test]
    fn explicit_secondary_table_uses_its_subobject_address() {
        unsafe {
            let ptr = fixture_new();
            {
                let view = &*ptr.cast::<DirectTables>();
                assert_eq!(view.left_value(), 13);
                assert_eq!(view.right_value(), 23);
            }
            fixture_delete(ptr);
        }
    }

    #[test]
    fn extending_a_single_base_reuses_its_vptr() {
        unsafe {
            let ptr = fixture_extended_left();
            assert_eq!(size_of::<ExtendedLeft>(), fixture_extended_left_size());
            assert_eq!((*ptr).left_value(), 10);
            assert_eq!(ExtendedLeft::left_extra(ptr, 5), 15);
            fixture_delete_extended_left(ptr);
        }
    }

    #[test]
    fn multiple_extended_bases_keep_layout_and_native_overrides() {
        unsafe {
            let ptr = fixture_extended();
            assert_eq!(size_of::<Extended>(), fixture_extended_size());
            assert_eq!(
                std::mem::offset_of!(Extended, right),
                fixture_extended_right_offset(ptr)
            );
            assert_eq!(Extended::left_extra(ptr, 5), 18);
            assert_eq!(Extended::right_extra(ptr, 7), 30);
            assert_eq!(fixture_extended_right_value(ptr), 27);
            fixture_delete_extended(ptr);
        }
    }

    #[test]
    fn child_extends_both_selected_base_tables_with_reference_receivers() {
        unsafe {
            let ptr = fixture_extended();
            assert_eq!(size_of::<ExtendedDirect>(), fixture_extended_size());
            {
                let view = &mut *ptr.cast::<ExtendedDirect>();
                assert_eq!(view.left_added(5), 18);
                assert_eq!(view.right_added(7), 30);
            }
            assert_eq!(fixture_extended_right_value(ptr), 27);
            fixture_delete_extended(ptr);
        }
    }
}
