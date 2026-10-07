// The UI is embedded from ui/dist. Make sure the folder exists so the server
// still compiles (and serves a hint page) before the UI has been built.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).expect("create ui/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>QRZero</title><p>The UI has not been built. Run <code>npm run build</code> in <code>ui/</code>.</p>",
        )
        .expect("write placeholder");
    }
    println!("cargo:rerun-if-changed=../../ui/dist");
}
