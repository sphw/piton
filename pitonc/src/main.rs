use std::path::PathBuf;

use clap::{Parser, ValueEnum};

fn main() -> miette::Result<()> {
    let args = Args::parse();
    match args.lang {
        Lang::Rust => piton_build::RustBuilder::default()
            .server()
            .client()
            .types()
            .build(&args.path),
        Lang::Cpp => piton_build::CppBuilder::default()
            .types()
            .out_dir(args.out)
            .build(&args.path),
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(value_enum)]
    lang: Lang,
    path: PathBuf,
    out: PathBuf,
}

#[derive(ValueEnum, Debug, Clone)]
enum Lang {
    Rust,
    Cpp,
}
