use std::env;
use winres::WindowsResource;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        WindowsResource::new()
            .set("FileVersion", "1.0.0")
            .set("ProductVersion", "1.0.0")
            .set("FileDescription", "Windows DLL loading diagnostics CLI")
            .set("ProductName", "loadwhat")
            .set("CompanyName", "loadwhat")
            .set("LegalCopyright", "")
            .set("OriginalFilename", "loadwhat.exe")
            .set("InternalName", "loadwhat")
            .compile()
            .expect("Failed to compile Windows resource");
    }
}
