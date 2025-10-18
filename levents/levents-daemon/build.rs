fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .expect("failed to locate vendored protoc binary");
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile(&["proto/events.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/events.proto");
    Ok(())
}
