
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

Overlapping slots and unaligned byte offsets are rejected for the target:

```compile_fail
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Overlap {
    #[vtable(first(&self); offset(0); second(&self);)]
    vtable: *const (),
}
```

```compile_fail
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Unaligned {
    #[vtable(offset(1); value(&self);)]
    vtable: *const (),
}
```

Extensions cannot overlap bound ancestor slots, including transitive bindings:

```compile_fail,E0080
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Base {
    #[vtable(value(this: *const Self) -> i32;)]
    vtable: *const (),
}
#[cpp_struct]
pub struct Child { #[inherit] base: Base }
#[cpp_struct]
pub struct Overlap {
    #[inherit]
    #[vtable(offset(0); other(this: *const Self) -> i32;)]
    base: Child,
}
```

An extension requires an inherited vptr:

```compile_fail,E0080
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Base { value: i32 }
#[cpp_struct]
pub struct Invalid {
    #[inherit]
    #[vtable(offset(0); value(this: *const Self) -> i32;)]
    base: Base,
}
```

Extensions require an explicit starting offset:

```compile_fail
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Base {
    #[vtable(value(this: *const Self) -> i32;)]
    vtable: *const (),
}
#[cpp_struct]
pub struct Invalid {
    #[inherit]
    #[vtable(other(this: *const Self) -> i32;)]
    base: Base,
}
```
