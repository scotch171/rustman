// Embeds the app icon into rustman.exe on Windows.
fn main() {
    println!("cargo:rerun-if-changed=assets/rustman.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/rustman.ico");
        res.compile().expect("failed to embed the Windows icon");
    }
}
