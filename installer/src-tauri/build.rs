fn main() { println!("cargo:rerun-if-env-changed=K11C_HELPER_SHA256"); tauri_build::build() }
