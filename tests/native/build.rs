//! Compile the native ABI validation fixture, not code shipped to macro consumers.
fn main() {
    println!("cargo:rerun-if-changed=fixture.cpp");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .extra_warnings(true)
        .warnings_into_errors(true)
        .file("fixture.cpp")
        .compile("ctables_fixture");
}
