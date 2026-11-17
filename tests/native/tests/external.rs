//! Cross-crate inherited dispatch validation.
#![allow(missing_docs, reason = "public native layout fixture")]
use rust_ctables::cpp_struct;
use rust_ctables_native_tests as fixture;

#[cpp_struct]
pub struct ExternalChild {
    #[inherit]
    pub base: fixture::Derived,
}

#[cfg(target_env = "msvc")]
#[cpp_struct]
pub struct ExtendedGrandchild {
    #[inherit]
    #[vtable(
        offset(0x20);
        grandchild_value(this: *const Self) -> i32;
    )]
    pub base: fixture::Extended,
}

#[cfg(not(target_env = "msvc"))]
#[cpp_struct]
pub struct ExtendedGrandchild {
    #[inherit]
    #[vtable(
        // Itanium also adds Extended::right_extra to the primary table.
        offset(0x28);
        grandchild_value(this: *const Self) -> i32;
    )]
    pub base: fixture::Extended,
}

unsafe extern "C" {
    fn fixture_new() -> *mut fixture::Derived;
    fn fixture_delete(value: *mut fixture::Derived);
    fn fixture_extended_grandchild() -> *mut ExtendedGrandchild;
    fn fixture_delete_extended_grandchild(value: *mut ExtendedGrandchild);
}

#[test]
fn extensions_cross_crates_and_extend_transitive_primary_tables() {
    unsafe {
        let ptr = fixture_extended_grandchild();
        assert_eq!(ExtendedGrandchild::grandchild_value(ptr), 33);
        assert_eq!(ExtendedGrandchild::left_extra(ptr, 5), 18);
        assert_eq!(ExtendedGrandchild::right_extra(ptr, 7), 30);
        assert_eq!(ExtendedGrandchild::grandchild_value(ptr), 40);
        fixture_delete_extended_grandchild(ptr);
    }
}

#[test]
fn metadata_crosses_crates_and_preserves_transitive_methods() {
    unsafe {
        let native = fixture_new();
        {
            let view = &mut *native.cast::<ExternalChild>();
            assert_eq!(view.left_value(), 13);
            assert_eq!(view.right_value(), 23);
            assert_eq!(view.apply(&fixture::Payload { amount: 7 }), 17);
            view.set_right(90);
            assert_eq!(view.right_value(), 93);
        }
        fixture_delete(native);
    }
}
