Blockhaven 0.3.2 — Windows x64

Run BlockhavenSetup.exe for installation and automatic update checks.
Setup is native Rust/Win32 and requires no WebView runtime.
Choose Desktop / Start menu shortcuts and optional WebView2 installation.
It installs to %AppData%\Blockhaven\launcher, copies itself and launches Blockhaven.
Both shortcuts target the installed updater.

For direct launching, run Blockhaven.exe with WebView2Loader.dll beside it.
The launcher requires Microsoft WebView2 Runtime and suitable Java.

Minecraft launch hides the launcher window. Game exit restores it.
Home and Instances offer instance/version folder shortcuts.
Discord Application ID: 1558468312172732539 (provided by default).
You can change it in Settings > Discord Rich Presence.
Desktop Discord must be running. Optional asset key: upload the bundled icon
to your Discord application before entering its asset name.

Full setup and build instructions: ../../README.md
This unsigned build needs a local Windows/Minecraft/Discord end-to-end check.
