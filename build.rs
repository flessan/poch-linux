// Embeds icon.ico (project root) into poch.exe and sets the name shown in Task Manager / Explorer.
fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set("ProductName", "Poch");
        res.set("FileDescription", "Poch");
        res.set("InternalName", "Poch");
        res.set("OriginalFilename", "poch.exe");
        if let Err(e) = res.compile() {
            // Do not break the build; the app still runs, only the exe icon would be missing.
            println!("cargo:warning=could not embed icon.ico into the exe: {e}");
        }
    }
}
