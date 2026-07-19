fn main() {
    glib_build_tools::compile_resources(
        &["assets"],
        "assets/icons.gresource.xml",
        "icons.gresource",
    );
}
