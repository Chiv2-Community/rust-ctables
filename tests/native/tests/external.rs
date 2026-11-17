//! Cross-crate inherited dispatch validation.
#![allow(missing_docs, reason = "public native layout fixture")]
use rust_ctables::cpp_struct;
use rust_ctables_native_tests as fixture;

#[cpp_struct]
pub struct ExternalChild {
    #[inherit]
    pub base: fixture::Derived,
}

unsafe extern "C" {
    fn fixture_new() -> *mut fixture::Derived;
    fn fixture_delete(value: *mut fixture::Derived);
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
