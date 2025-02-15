use std::{fs, io};
use std::path::Path;

const PROTO_INCLUDE_DIRS: &[&str] = &["proto"];
const PROTO_SOURCE_FILES: &[&str] = &["file_transfer.proto"];
const GENERATED_PROTO_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/proto");

fn main() -> io::Result<()> {
    let out_dir: &Path = Path::new(GENERATED_PROTO_DIR);
    if !out_dir.exists() {
        fs::create_dir_all(out_dir)?;
    }

    prost_build::Config::new()
        .out_dir(out_dir)
        .compile_protos(PROTO_SOURCE_FILES, PROTO_INCLUDE_DIRS)?;

    println!("cargo:rustc-env=GENERATED_PROTO_DIR={}", out_dir.display());

    Ok(())
}
