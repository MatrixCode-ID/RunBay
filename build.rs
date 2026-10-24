//! Compiles the Slint UI and embeds the Windows icon and version information.

fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("material".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let mut res = winresource::WindowsResource::new();
        res.set_icon("ui/logo/runbay.ico")
            .set("ProductName", "RunBay")
            .set("FileDescription", "RunBay")
            .set("OriginalFilename", "RunBay.exe")
            .set("LegalCopyright", "MIT License")
            .set("ProductVersion", &version)
            .set("FileVersion", &version);
        res.compile().expect("cannot embed Windows resources");
    }
    println!("cargo:rerun-if-changed=ui/logo/runbay.ico");
}
