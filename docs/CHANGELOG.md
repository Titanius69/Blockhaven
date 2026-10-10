# 0.3.2

- Open Windows instance/version folders through the native Shell API with normalized absolute paths, Unicode support and reported errors.
- Add GitHub description/summary and a comprehensive source repository `.gitignore`.
- List manually added JARs and disabled JARs in Installed mods, with a Manual badge and enable/disable switches.
- Refresh on opening, explicit Refresh and periodic polling; include manual files in counts.
- Exclude managed filenames, directories and symlinks, preserve collisions and block toggling while Minecraft runs.

# 0.3.1

- Default Discord Application ID: `1558468312172732539`.
- Migrate blank/missing IDs while preserving custom IDs and disabled presence. Clearing the field resets it to the default.

# 0.3.0

- Hide the native window after the Java game process starts; restore, unminimize and focus it on normal exit, crash or stop.
- Preserve visibility commands across full UI redraws and very short game lifetimes; failed startup does not hide the launcher.
- Add Home and Instances shortcuts to the instance folder and its Vanilla/Fabric installation version folder.
- Resolve folder paths from saved instances and reject traversal/symlinked managed paths.
- Add configurable Discord local IPC Rich Presence, launcher/game activity, version/loader, play timer, optional artwork, reconnects, status feedback and activity clearing.
- Load older settings with default optional Discord configuration. Avoid Java/Explorer console flashes on Windows.
- Restore missing Cargo/build/native-shell/test files around the newly supplied application sources and artwork.
- Include Windows x64 release executable and WebView2 loader, plus source and tests.
