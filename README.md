# rust-ctables

Explicit Rust views of native C++ objects. `#[cpp_struct]` generates private
vtables and unsafe, always-inlined forwarding methods. It does not construct
C++ objects or infer their layouts.

```rust
use rust_ctables::cpp_struct;

#[cpp_struct]
pub struct Left {
    #[vtable(
        pad(5);
        value(&self) -> i32;
        set_value(&mut self, value: i32);
    )]
    vtable: *const (),
    pub value: i32,
    reserved: i32,
}

#[cpp_struct]
pub struct Right {
    #[vtable(other_value(&self) -> i32;)]
    vtable: *const (),
    pub value: i32,
    reserved: i32,
}

#[cpp_struct]
pub struct Derived {
    #[inherit]
    pub left: Left,
    #[inherit]
    pub right: Right,
    pub extra: i32,
}
```

`Derived` gets `value`, `set_value` and `other_value` methods. Calls use the
corresponding parent-subobject address. Native vtables provide overrides and
secondary-base adjustment thunks. Transitive inheritance and parents in other
modules or crates are supported. Conflicting method names are compile errors;
declare such parent fields without `#[inherit]` and call them explicitly.
Use the parent's defining module path (for example `bindings::Left`) when it
is outside the child's module: importing only the type does not import its
metadata, and renamed type aliases are not resolved by the macro.

## Declaration rules

- `#[repr(C)]`, `unsafe extern "C"` slots and `#[inline(always)]` methods are implied.
- Vtable fields must be private `*const ()` placeholders. Expansion replaces
  each with a pointer to a generated private table. Other fields stay unchanged.
- `pad(n)` advances by `n` pointer-sized slots; `offset(n)` sets the cursor to
  byte offset `n` from this vtable's start, not a process address. Hex literals
  such as `offset(0x150)` are supported. Every method advances by one slot.
  Methods may appear in any order: expansion sorts them by offset, fills gaps,
  and rejects overlaps, misalignment and arithmetic overflow for the target's
  pointer width. `offset(0)` returns to the first slot; `pad(0)` is rejected.
- Multiple vtable fields are allowed at their declared positions. Their cursors
  are independent; table-relative offsets do not move the object's pointer fields.
- Entries omit `fn` and `unsafe`. Generated forwarding methods are always public
  and unsafe. Document native preconditions under a standard Rustdoc `# Safety`
  heading on entries; the generated method keeps that section and includes the
  common native-object/ABI contract without adding another heading.
- Receivers may be `&self`, `&mut self`, `this: *const Self` or `this: *mut Self`.
  Raw receivers generate associated functions and never construct references.
- Keep native-to-Rust conversions and ownership in handwritten wrappers.
- Generic structs, virtual inheritance, conditional vtable/inherit fields and
  explicit `repr` attributes are rejected or unsupported.
- Use the dependency name `rust-ctables` (Rust path `rust_ctables`), without renaming.
- Apply `#[cpp_struct]` before derives. Marker attributes are consumed before
  Rust checks the struct; rustfmt can parse the ordinary field syntax.

## Safety and ABI limits

This is not a portable C++ ABI translator. Validate sizes, alignments, offsets,
slot indices and calling conventions against the exact compiler and binary.
The C calling convention is suitable for the tested x64 bindings; it is not
MSVC x86 `thiscall`. Destructors, aggregate returns, virtual bases and other ABI
special cases need separate analysis. Skipped entries need not be functions.

Rust `repr(C)` composition cannot express all C++ base layouts: some ABIs reuse
base tail padding or optimize empty bases. Explicit padding fields do not
automatically fix every such layout. Only use `#[inherit]` where the complete
parent subobject occupies the declared Rust field. For mismatching layouts,
write a verified prefix/view instead.

Callers must ensure object/vtable liveness, initialized slots, correct function
signatures, native argument contracts, thread rules and reference aliasing.
The macro performs no runtime validation and confers no ownership or GC pinning.
Use raw receivers for mutable native state beyond a modeled prefix, such as
shared allocators. The caller must still provide a valid complete allocation.

```rust
use rust_ctables::cpp_struct;
#[cpp_struct]
pub struct Allocator {
    #[vtable(
        /// Release cached memory.
        ///
        /// # Safety
        /// Must run on the main thread.
        trim(this: *mut Self, trim_thread_caches: bool);
    )]
    vtable: *const (),
}
```

`# Safety` is a documentation convention checked by Clippy's `missing_safety_doc`
lint, not a language-level proof or a runtime check. Ordinary entry documentation
stays ordinary documentation; the macro does not infer safety rules from prose.
Entry docs and safety requirements also propagate to inherited methods. Use
literal `///` comments or `#[doc = "..."]` for heading merging; headings supplied
by expressions such as `include_str!` cannot be inspected by this macro.

## Validation

```sh
cargo test --workspace --locked
cargo test --workspace --locked --release
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The separate native-test package compiles C++ objects and validates layout,
primary/secondary overrides, inherited calls, cross-crate metadata, transitive
inheritance and raw receivers. C++ fixture compilation is not a dependency of
macro consumers. CI runs the same tests on Linux and Windows MSVC x64.

## Tooling

Rust 1.93.0, rustfmt and Clippy are pinned in `rust-toolchain.toml`.
`cargo fmt --all` uses the checked-in formatter settings; VS Code formats Rust
on save. CI caches Cargo dependencies and build artifacts per OS/toolchain and
invalidates them on manifest, lockfile or compiler changes. Only `main` saves
shared caches; pull requests may restore them.

Workspace lints deny compiler warnings, missing public documentation,
future-incompatible/edition-compatibility issues, and Clippy's `all`, `pedantic`
and `nursery` groups. Unsafe operations inside unsafe functions require explicit
blocks. The macro implementation forbids unsafe Rust. Release builds retain
overflow checks and debug assertions. Narrow test/code-generation exceptions
carry reasons; conflicting Clippy restriction lints are not enabled wholesale.
