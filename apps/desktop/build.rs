fn main() {
    for asset in [
        "clipcraft.res",
        "clipcraft.rc",
        "clipcraft.ico",
        "capture-controller.ico",
    ] {
        println!("cargo:rerun-if-changed=assets/brand/{asset}");
    }
    // The checked-in, architecture-neutral resource is generated from the ICO
    // with the Windows SDK. Normal offline builds need no resource compiler.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let resource = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("assets/brand/clipcraft.res");
        println!("cargo:rustc-link-arg-bins={}", resource.display());
    }
}
