//! Compiles the interface, gives the executable its icon, and supplies the one
//! Windows import library the GNU toolchain leaves out.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Import libraries the bundled mingw-w64 does not carry.
const MISSING: &[&str] = &["libshlwapi.a"];

/// The icon shown for the executable in a file listing and on the taskbar.
const ICON: &str = "../../assets/icons/ferrum-grid.ico";

fn main() {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed={ICON}");

    slint_build::compile("ui/main.slint").expect("the interface should compile");
    supply_missing_import_libraries();
    embed_executable_icon();
}

/// Attach the icon and the version details to the executable.
///
/// Windows reads these from a resource compiled into the binary rather than
/// from anything at runtime, so this is the only way an `.exe` gets a picture
/// in Explorer. A missing resource compiler is not an error: the application
/// still builds and still shows its icon in its own window, which comes from
/// the interface rather than from here.
fn embed_executable_icon() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let icon = PathBuf::from(ICON);
    if !icon.exists() {
        println!("cargo:warning=no icon at {ICON}; the executable will have none");
        return;
    }
    let Ok(icon) = fs::canonicalize(&icon) else {
        return;
    };

    // The resource compiler wants forward slashes, and canonicalize hands back
    // a verbatim path that it does not understand.
    let icon_path = icon
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let comma_version = {
        let mut parts: Vec<&str> = version.split('.').collect();
        parts.resize(4, "0");
        parts.join(",")
    };

    let script = format!(
        r#"1 ICON "{icon_path}"

1 VERSIONINFO
FILEVERSION {comma_version}
PRODUCTVERSION {comma_version}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "FerrumGrid"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "FerrumOffice"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "ferrum-grid.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 0x04B0
  END
END
"#
    );

    let out = PathBuf::from(out_dir);
    let script_path = out.join("ferrum-grid.rc");
    let object_path = out.join("ferrum-grid-resource.o");
    if fs::write(&script_path, script).is_err() {
        return;
    }

    let Some(windres) = find_on_path("windres.exe") else {
        println!("cargo:warning=windres not found; the executable will have no icon");
        return;
    };

    let compiled = Command::new(windres)
        .arg("--input")
        .arg(&script_path)
        .arg("--output")
        .arg(&object_path)
        .arg("--output-format=coff")
        .status();

    match compiled {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bins={}", object_path.display());
        }
        _ => println!("cargo:warning=the resource compiler failed; no executable icon"),
    }
}

/// Copy the libraries the toolchain lacks into the build directory, and put
/// only that directory on the link path.
///
/// Adding a full MinGW installation's `lib` directory instead would let the
/// linker resolve every other system import library from there too, mixing two
/// toolchains' idea of the Windows API in one binary. That produces something
/// that dies during loader initialisation with a stack overflow and no output
/// at all, which is a memorably unhelpful way to be told. Copying the one file
/// that is actually missing keeps everything else coming from the toolchain
/// doing the build.
fn supply_missing_import_libraries() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return;
    }
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let shim = PathBuf::from(out_dir).join("import-libs");

    let mut supplied = false;
    for name in MISSING {
        let destination = shim.join(name);
        if destination.exists() {
            supplied = true;
            continue;
        }
        let Some(source) = find_beside_gcc(name) else {
            continue;
        };
        if fs::create_dir_all(&shim).is_ok() && fs::copy(&source, &destination).is_ok() {
            supplied = true;
        }
    }

    if supplied {
        println!("cargo:rustc-link-search=native={}", shim.display());
    }
}

/// Look for a library in a MinGW installation on the path.
///
/// Located rather than hardcoded, so the repository holds no path belonging to
/// one machine.
fn find_beside_gcc(name: &str) -> Option<PathBuf> {
    for bin in toolchain_directories() {
        let Some(root) = bin.parent() else {
            continue;
        };
        let candidates = [
            root.join("x86_64-w64-mingw32").join("lib").join(name),
            root.join("lib").join(name),
        ];
        if let Some(found) = candidates.iter().find(|p| Path::exists(p)) {
            return Some(found.clone());
        }
    }
    None
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.exists())
}

/// Directories on the path that hold a `gcc`.
fn toolchain_directories() -> Vec<PathBuf> {
    let Some(path) = env::var_os("PATH") else {
        return Vec::new();
    };
    env::split_paths(&path)
        .filter(|dir| dir.join("gcc.exe").exists())
        .collect()
}
