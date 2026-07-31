fn main() {
    println!("cargo:rerun-if-changed=blender_scripts/normalize_v1.py");

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon/aio-asset-normalizer.ico");

        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/icon/aio-asset-normalizer.ico");
        resource
            .compile()
            .expect("failed to embed the application icon");
    }
}
