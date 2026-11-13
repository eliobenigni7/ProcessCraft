//! Windows: embed the app icon and version info in processcraft.exe (no-op elsewhere).
fn main() {
    println!("cargo::rerun-if-changed=../../assets/app-icon/processcraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/app-icon/processcraft.ico");
        res.set("ProductName", "ProcessCraft");
        res.set("FileDescription", "ProcessCraft");
        if let Err(e) = res.compile() {
            println!("cargo::warning=winresource: {e}");
        }
    }
}
