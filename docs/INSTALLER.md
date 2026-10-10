# Native installer and updater

`BlockhavenSetup.exe` is a Windows x64 GUI written in Rust with native Win32 controls, GDI drawing and Windows Shell shortcuts. It has no WebView, HTML, JavaScript, browser process or external GUI runtime dependency. The launcher continues to use WebView2.

## Install

Run `bin/windows-x64/BlockhavenSetup.exe` anywhere on your PC. Choose:

- **Create a desktop shortcut**
- **Add Blockhaven to the Start menu**
- **Install Microsoft WebView2 Runtime if missing**

Click **Install & launch**. Setup downloads the launcher specified by the online manifest, installs its loader, copies itself and creates the selected shortcuts. If WebView2 is missing and its checkbox is selected, setup downloads Microsoft's Evergreen bootstrapper and runs `/silent /install`, then verifies runtime detection before starting Blockhaven. An existing runtime is kept.

If WebView2 is missing and its checkbox is off, Blockhaven, the updater and chosen shortcuts are still installed. Setup explains the missing dependency and waits for you to enable runtime installation or install it yourself. It does not silently install a runtime you declined.

Both shortcuts target the installed **updater**, so subsequent launches check for updates automatically. You can delete the original downloaded installer after setup. Run the installed updater with `--settings` to change shortcuts/runtime preferences without immediately starting the update check. Unchecking a shortcut option removes only a shortcut targeting this updater.

## Files

Everything belonging to the updater is under **`%AppData%\Blockhaven\launcher`**:

| File | Purpose |
| --- | --- |
| `BlockhavenSetup.exe` | Installed native updater; shortcut target |
| `blockhaven.exe` | Downloaded launcher |
| `manifest.txt` | Last successfully installed release metadata |
| `installer.json` | Desktop, Start menu and runtime preferences |
| `WebView2Loader.dll` | Bundled Windows x64 loader for GNU launcher releases |

Launcher settings and game storage remain at their existing locations. Updates do not delete accounts, instances, worlds or mods. The Runtime and Java are separate system/user installations; this installer handles the optional Runtime, not Java.

## Update behavior

Manifest URL:

```text
https://raw.githubusercontent.com/Titanius69/Blockhaven/refs/heads/main/manifest.txt
```

Supported format (the supplied endpoint currently uses the first two fields):

```text
version = 0.3.2
url = https://github.com/Titanius69/Blockhaven/releases/download/0.3.2/blockhaven.exe
```

An optional `sha256 = <64 hexadecimal characters>` adds content verification. Whitespace, comments and line endings do not affect matching. Version, URL and optional hash are compared to the installed manifest. A mismatch, missing executable or invalid executable triggers a download. Equal metadata and a valid installed binary skip it. The remote manifest is authoritative, so an intentional version rollback also installs that version.

Downloads use HTTPS, bounded size/timeouts and restricted redirect hosts. The launcher URL must be a release executable in `Titanius69/Blockhaven`. Downloads are staged next to the destination and checked for complete Windows x64 executable headers/sections and any supplied SHA-256 before replacement. The local manifest is committed after the binary; a metadata write failure rolls the binary back. Failed/truncated downloads preserve the installed version. If the launcher is locked/running, close it and retry. Only one updater window can operate on an installation at a time.

On network failure, **Launch installed version** starts an intact local installation without an update check. It never falls back to an incomplete download. Cancel stops a download before replacement; Microsoft's Runtime installation is allowed to finish safely before cancellation takes effect.

The updater itself is replaced when you run a newer downloaded setup executable. The two-field launcher manifest does not remotely update setup itself.

## Build both applications

From the workspace root, on Windows with stable Rust and MSVC Build Tools or MinGW:

```sh
cargo build --release --locked
```

Outputs:

```text
target/release/blockhaven.exe
target/release/BlockhavenSetup.exe
```

Both packages are workspace default members. Setup alone can be built without any launcher/WebView dependencies:

```sh
cargo build -p blockhaven-installer --release --locked
```

The bundled x64 WebView2 loader and SDK license are under `crates/blockhaven-installer/runtime/windows-x64`. They must stay in the repository. Do not omit the DLL because it is a build input for the installer. The updater does not load this DLL; it writes it alongside the launcher.

## Publish releases

1. Upload the launcher as **`blockhaven.exe`**, matching the case-sensitive GitHub release URL.
2. Upload **`BlockhavenSetup.exe`** for first-time installation/updater replacement.
3. Commit the corresponding `version` and `url` to root `manifest.txt` on `main` after the launcher asset is available. Add its SHA-256 if desired.

Setup always uses the live manifest, not the launcher executable included beside it in this ZIP.

## Validation

The package passes 12 installer unit/integration tests covering manifest comparison, repair, rollback, corrupt downloads, checksum mismatch, cancellation, self-copy, options and conditional runtime installation. Two explicit network tests verified the live GitHub manifest/release and Microsoft's bootstrapper redirect/header without executing either download. Both Windows release binaries compile; setup's PE imports contain no WebView2 loader, and its dependency tree contains no Wry/Tao/WebView crate. Native Windows shortcut creation, runtime installation and graphical interaction still need a local Windows check.

References: [Microsoft WebView2 deployment](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution), [Windows Shell links](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ishelllinkw), [Known folders](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath).
