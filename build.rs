fn main() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(dir) = std::env::var("DEP_SYPHON_FRAMEWORK_DIR") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
        }
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
}
