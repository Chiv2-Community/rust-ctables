//! Native ABI fixtures; public declarations are consumed by cross-crate tests.
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
            left_value(&self) -> i32;
            untouched(&self) -> i32;
            apply(&self, payload: *const Payload) -> i32;
        )]
        vtable: *const (),
        pub x: i32,
        reserved: i32,
    }

    #[cpp_struct]
    pub struct Right {
        #[vtable(
            right_value(&self) -> i32;
            set_right(&mut self, value: i32);
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
}
