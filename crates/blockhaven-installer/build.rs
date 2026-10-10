fn main() {
    println!("cargo:rerun-if-changed=../../resources/icons/blockhaven.ico");
    println!("cargo:rerun-if-env-changed=BLOCKHAVEN_RC_TOOLKIT");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        assert_eq!(
            std::env::var("CARGO_CFG_TARGET_ARCH").unwrap(),
            "x86_64",
            "The bundled launcher loader is Windows x64"
        );
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../resources/icons/blockhaven.ico")
            .set("ProductName", "Blockhaven Setup")
            .set("FileDescription", "Blockhaven Native Installer and Updater")
            .set("FileVersion", "0.1.0.0")
            .set("ProductVersion", "0.1.0.0")
            .set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<assemblyIdentity version="0.1.0.0" processorArchitecture="amd64" name="Blockhaven.Setup" type="win32"/>
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo>
<application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware></windowsSettings></application>
</assembly>"#);
        if let Ok(path) = std::env::var("BLOCKHAVEN_RC_TOOLKIT") {
            res.set_toolkit_path(&path)
                .set_windres_path(&format!("{path}/x86_64-w64-mingw32-windres"))
                .set_ar_path(&format!("{path}/x86_64-w64-mingw32-ar"));
        }
        res.compile().expect("embed installer icon and manifest");
    }
}
