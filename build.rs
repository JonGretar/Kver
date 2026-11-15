use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icons/kver.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var_os("CARGO_FEATURE_GUI").is_none()
    {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "Kver's Windows resources require the MSVC Windows SDK"
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
        .join("assets/icons/kver.ico");
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    let numbers = ["MAJOR", "MINOR", "PATCH"]
        .map(|part| env::var(format!("CARGO_PKG_VERSION_{part}")).expect("Cargo version component"))
        .join(",");
    // Add only icon/version resources. GPUI owns RT_MANIFEST (DPI, controls,
    // privileges, compatibility); replacing it silently changes native behavior.
    let resource = format!(
        r#"#include <windows.h>
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "JonGretar\0"
      VALUE "FileDescription", "Kver PDF reader\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "kver\0"
      VALUE "OriginalFilename", "kver.exe\0"
      VALUE "ProductName", "Kver\0"
      VALUE "ProductVersion", "{version}\0"
      VALUE "LegalCopyright", "Kver contributors; MIT license\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon.to_string_lossy().replace('\\', "/")
    );
    let rc = out.join("kver.rc");
    let res = out.join("kver.res");
    fs::write(&rc, resource).expect("write Windows resources");
    let status = Command::new("rc.exe")
        .arg("/nologo")
        .arg("/fo")
        .arg(&res)
        .arg(&rc)
        .status()
        .expect("run rc.exe from an x64 MSVC developer shell with the Windows SDK");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=kver={}", res.display());
}
