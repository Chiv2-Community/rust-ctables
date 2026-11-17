
## Compile-time guarantees

Virtual dispatch requires an unsafe call:

```compile_fail,E0133
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Native {
    #[vtable(value(&self) -> i32;)]
    vtable: *const (),
}
fn unchecked(value: &Native) { value.value(); }
```

Conflicting inherited names are rejected:

```compile_fail
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Parent {
    #[vtable(value(&self) -> i32;)]
    vtable: *const (),
}
#[cpp_struct]
pub struct Ambiguous {
    #[inherit]
    left: Parent,
    #[inherit]
    right: Parent,
}
```

Vtable pointers remain private:

```compile_fail,E0616
mod bindings {
    use rust_ctables::cpp_struct;
    #[cpp_struct]
    pub struct Native {
        #[vtable(value(&self) -> i32;)]
        vtable: *const (),
    }
}
fn expose(value: &bindings::Native) { let _ = value.vtable; }
```
