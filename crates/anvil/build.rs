//! Встраивает значок и сведения о версии в anvil.exe.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/anvil.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon/anvil.ico");
        res.set("ProductName", "Anvil");
        res.set("FileDescription", "Anvil IDE");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=не удалось встроить значок: {e}");
        }
    }
}
