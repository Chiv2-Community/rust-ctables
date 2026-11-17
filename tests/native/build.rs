//! Compile the native ABI validation fixture, not code shipped to macro consumers.
fn main() {
    println!("cargo:rerun-if-changed=fixture.cpp");
    cc::Build::new()
        .cpp(true)
        .file("fixture.cpp")
        .compile("ctables_fixture");
}
