fn main() {
    piton_build::RustBuilder::default()
        .server()
        .client()
        .types()
        .build("./foo.kdl")
        .unwrap()
}
