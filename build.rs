fn main() {
    println!("cargo:rerun-if-changed=lang");

    // .exeのアイコン埋め込み (Windowsのみ。ui/ico/app.icoは16〜256pxのマルチサイズ)
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=ui/ico/app.ico");
        winresource::WindowsResource::new()
            .set_icon("ui/ico/app.ico")
            .compile()
            .expect("Failed to embed exe icon");
    }

    let config = slint_build::CompilerConfiguration::new()
        .with_style("native".into())
        .with_bundled_translations("lang");
    slint_build::compile_with_config("ui/app-window.slint", config).expect("Slint build failed");
}
