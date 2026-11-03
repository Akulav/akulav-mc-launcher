fn main() {
    // Embed the launcher icon and version info into the Windows executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/mining.ico");
        res.set("ProductName", "Akulav Launcher");
        res.set("FileDescription", "Akulav Launcher - made by Akulav");
        res.set("CompanyName", "Akulav");
        res.set("LegalCopyright", "© Akulav. All rights reserved.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
