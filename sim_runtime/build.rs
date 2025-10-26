use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc_path);

    let proto_dir = PathBuf::from("proto");
    println!(
        "cargo:rerun-if-changed={}",
        proto_dir.join("command.proto").display()
    );

    prost_build::Config::new().compile_protos(&[proto_dir.join("command.proto")], &[proto_dir])?;

    Ok(())
}
