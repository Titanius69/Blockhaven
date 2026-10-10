#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    if let Err(error) = blockhaven_installer::native::run() {
        blockhaven_installer::native::show_error(&error);
        std::process::exit(1);
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("Blockhaven Setup is a native Windows x64 application.");
        std::process::exit(1);
    }
}
