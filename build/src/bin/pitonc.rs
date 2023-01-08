fn main() -> miette::Result<()> {
   let args: Vec<String> = std::env::args().collect();
    piton_build::RustBuilder::default()
        .server()
        .client()
        .types()
        .build(&args[1])
}
