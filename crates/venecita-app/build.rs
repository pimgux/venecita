//! Windows executable resources. Does nothing on other targets.
//!
//! * The application manifest: per-monitor DPI awareness (v2), so window positions are exact
//!   physical pixels on every monitor, plus the usual modern defaults (common controls v6,
//!   Windows 10/11 compatibility).
//! * The icon and version information (product name shown in Explorer and Task Manager).
//!   These need the Windows resource compiler, so they are only added when building on
//!   Windows; a cross-check from another system skips them.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::manifest::DpiAwareness;
        embed_manifest::embed_manifest(
            embed_manifest::new_manifest("VenecitaDock").dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("unable to embed the Windows manifest");

        #[cfg(windows)]
        {
            let mut resource = winresource::WindowsResource::new();
            resource
                .set_icon("assets/icons/AppIcon.ico")
                .set("ProductName", "Venecita Dock")
                .set("FileDescription", "Venecita Dock")
                .set("CompanyName", "Venecita Dock contributors")
                .set("LegalCopyright", "GPL-3.0-or-later");
            resource
                .compile()
                .expect("unable to embed the Windows icon");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icons/AppIcon.ico");
}
