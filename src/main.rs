#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
fn main() {
    if let Err(error) = blockhaven::run() {
        eprintln!("Blockhaven: {error}");
        #[cfg(feature = "native")]
        rfd::MessageDialog::new()
            .set_title("Blockhaven")
            .set_description(error.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}
