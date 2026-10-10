# Blockhaven 0.3.2

A compact native Minecraft Java Edition launcher built with Rust and HTML/CSS, featuring multiple versions, offline accounts, Modrinth integration, mod management, and Discord Rich Presence.

Separate Vanilla and Fabric instances, a compact English interface, manual mod controls and automatic hide/restore while Minecraft runs keep everyday launching straightforward. Ready-to-paste GitHub copy is in [docs/GITHUB.md](docs/GITHUB.md).

## New behavior

- **Manual mods:** Installed mods also lists regular `.jar` and `.jar.disabled` files copied into the selected instance’s `mods` directory. They are marked Manual, with enable/disable switches. The list refreshes on navigation, on Refresh, and every two seconds while open. File renaming preserves the contents; manual files are not added to the Modrinth database. Managed filenames are excluded to prevent duplicate rows. Changing mods is blocked while Minecraft runs.

- **Hide while playing:** the native launcher window hides only after the Java game process starts. When that process exits, crashes or is stopped, the same window reappears, unminimizes and requests focus. Failed installation or Java startup keeps the launcher visible. Hidden-window monitoring uses the Rust/OS event loop, independent of browser timers.
- **Instance folder and Version folder:** shortcuts on Home and in Instances open the selected instance's own directory or its installation/loader version directory. Each instance card also has its own shortcuts. These work before installation, creating the directory if necessary. The launcher resolves stored instance IDs; it does not accept arbitrary folder paths from the interface.
- **Discord Rich Presence:** optional local IPC integration, with reconnect attempts when Discord starts later or restarts. It displays launcher activity while browsing and Minecraft version, Vanilla/Fabric and elapsed play time while the game runs. Disabling it or closing the launcher clears its activity. No Discord bot token, account login, player identity or authentication tokens are sent.

The compact English interface, Modrinth project artwork/cache, offline accounts, separate instances, version selection, Fabric and managed-mod operations from the supplied source are retained. Existing configurations without Discord settings remain readable.

## Run on Windows

Extract the ZIP and open **`bin/windows-x64/Blockhaven.exe`**. Keep `WebView2Loader.dll` next to it. Microsoft WebView2 Runtime and an appropriate Java runtime must be installed separately. Minecraft and mods are not bundled.

1. Add an offline account in **Accounts**, or configure your own Microsoft OAuth client ID and sign in.
2. Choose `java` / `java.exe` in **Settings** and check it. Each instance can override Java.
3. Create an instance with a Minecraft version and Vanilla or Fabric loader.
4. Install Minecraft, then launch. The launcher hides while the game process runs and returns when it exits.
5. For Fabric, use **Discover mods** to select a compatible Modrinth version. Required dependencies are resolved automatically.

Offline accounts work in single-player and on servers that allow offline identities. Initial installation and new downloads need internet access. Authenticated servers require an eligible online account. Microsoft sign-in needs a registered/approved public client ID of your own; online tokens remain in memory.

## Enable Discord

The default **Application ID is `1558468312172732539`** and activity sharing is enabled by default. Start the desktop Discord client with activity sharing enabled; no Application ID entry is needed. Settings reports connection/retry status.

Existing configurations with a blank or missing ID use this default. Custom IDs and the disabled setting are preserved. Clearing the ID in Settings resets it to the default.

You can enter a different ID in **Settings → Discord Rich Presence**. To use your own application, create one in the [Discord Developer Portal](https://discord.com/developers/applications) and copy its Application ID. Optional artwork: upload `resources/icons/blockhaven.png` in that Discord application as `blockhaven`, then enter that artwork key. Leave the artwork field empty for text-only presence.

Presence reports the game process/version/loader and play duration. It cannot report your world, server or dimension without an in-game integration. It also works with offline Minecraft accounts. Browser-only Discord has no local IPC socket.

## Folder locations

| Shortcut | Location under the configured game storage |
| --- | --- |
| Instance folder | `instances/<instance UUID>` — worlds, options, mods |
| Version folder, Vanilla | `versions/<Minecraft version>` |
| Version folder, Fabric | `versions/<Minecraft version>-fabric-<loader version>` — installation receipt and native libraries |

Minecraft assets and dependency libraries use shared caches. Changing the storage setting does not move existing files. Removing an instance from the launcher keeps its files/worlds.

On Windows, folder shortcuts use the native Shell open operation with a normalized absolute path, preserving spaces and Unicode. Missing directories are created first; Shell errors are displayed in the launcher.

## Build

Install stable Rust and your platform's native development tools. Run from the folder containing `Cargo.toml`:

```sh
cargo run --locked
cargo build --release --locked
```

Windows: MSVC Build Tools, or Rust GNU with MinGW. The ICO and version information are embedded at build time. GNU builds need the x64 `WebView2Loader.dll` from `target/release/build/webview2-com-sys-*/out/x64` alongside the executable. The supplied build includes it.

Ubuntu/Debian: install `build-essential`, `pkg-config`, `libgtk-3-dev` and `libwebkit2gtk-4.1-dev`. macOS: install Xcode Command Line Tools. Linux/macOS sources are included; native behavior on those desktops needs local verification.

No Node, bundler, WASM or local HTTP server is needed by the application. Java and the system WebView runtime remain platform dependencies. `cargo run --features dev` enables disk-backed CSS and WebView devtools; Rust changes require rebuilding.

## Implementation and checks

The frontend is HTML/CSS. Views, accounts, launch/install operations, settings, folder opening, window visibility and Discord RPC are Rust. The framework's small `bridge.js` only forwards generic DOM events and applies Rust-produced patches. Visibility commands go directly to the native window; they never depend on JavaScript execution while hidden.

`src/presence.rs` connects to Discord's local named pipe on Windows or Unix socket on Linux/macOS. It uses bounded frames and read/write deadlines, acknowledges keepalive frames, checks activity acknowledgements, retries connection failures and clears activity on shutdown. It does not contact Discord HTTP APIs.

```sh
cargo test --workspace --no-default-features --locked
cargo build --no-default-features --locked
python3 tests/scripts/desktop_flow.py
# Optional test-only dependency; not required to build/run the app:
# npm install --no-save jsdom
node tests/scripts/bridge.cjs
```

Validated for this package: 30 Rust tests; real child-process stop/crash visibility transitions; dispatcher tests for offline launch, folder creation, Discord settings, hide/restore and failed Java startup; generic DOM/IPC event checks; Clippy with warnings denied; Windows x64 release compilation and executable icon/version resources. Discord protocol tests use a local simulated Discord socket. A Windows graphical session, a complete Minecraft game and live desktop Discord could not be run in this environment and need a local end-to-end check.

Official metadata fixtures were restored from the Mojang/Fabric/Modrinth APIs. The fixture catalog can be refreshed in the launcher. Forge, NeoForge, Quilt, Bedrock and modpack import are outside the supplied implementation.

MIT source license. Third-party notices, including the WebView2 SDK license, are in `docs/THIRD_PARTY_NOTICES.txt`. This is an independent project, not an official Mojang, Microsoft or Discord product.

References: [Mojang manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json), [Fabric Meta](https://meta.fabricmc.net/), [Modrinth API](https://docs.modrinth.com/api/), [Discord RPC protocol](https://github.com/discord/discord-rpc/blob/master/documentation/hard-mode.md), [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
