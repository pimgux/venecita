//! Embeds the Windows application manifest: per-monitor DPI awareness (v2), so window
//! positions are exact physical pixels on every monitor, plus the usual modern defaults
//! (common controls v6, Windows 10/11 compatibility). Does nothing on other targets.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::manifest::DpiAwareness;
        embed_manifest::embed_manifest(
            embed_manifest::new_manifest("VenecitaDock").dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("unable to embed the Windows manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
