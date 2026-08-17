fn main() {
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/icons/app.ico")
        .compile()
        .expect("failed to embed the Windows application icon");
}
