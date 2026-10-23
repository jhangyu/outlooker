use std::{env, fs, path::Path, path::PathBuf, process::Command};

/// The 17 local libyal libraries compiled with libpff (spec Part 01 §4.3).
const LOCAL_LIBS: [&str; 17] = [
    "libbfio", "libcdata", "libcerror", "libcfile", "libclocale", "libcnotify", "libcpath", "libcsplit",
    "libcthreads", "libfcache", "libfdata", "libfdatetime", "libfguid", "libfmapi", "libfvalue", "libfwnt", "libuna",
];

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendored = manifest.join("../../native/libpff");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", vendored.join("ORIGIN").display());
    assert!(vendored.join("ORIGIN").exists(), "src-tauri/native/libpff is missing: run scripts/vendor-libpff.py");

    let msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc";
    let src = PathBuf::from(env::var("OUT_DIR").unwrap()).join("libpff-src");
    let _ = fs::remove_dir_all(&src);
    copy_dir(&vendored, &src);
    if !msvc {
        // Configure in the copy: the sdist ships a Linux common/config.h that common/common.h
        // includes from its own directory first (spec Part 01 §4.3).
        let status = Command::new("sh")
            .args(["configure", "--disable-nls", "--disable-shared-libs", "--with-zlib=no"])
            .current_dir(&src)
            .status()
            .expect("failed to run libpff configure");
        assert!(status.success(), "libpff configure failed, see {}", src.join("config.log").display());
    }

    let mut dirs = vec!["libpff"];
    dirs.extend(LOCAL_LIBS);
    let mut build = cc::Build::new();
    build.warnings(false).include(&src).include(src.join("common")).include(src.join("include"));
    for dir in &dirs {
        build.include(src.join(dir));
    }
    for lib in LOCAL_LIBS {
        build.define(&format!("HAVE_LOCAL_{}", lib.to_uppercase()), None);
    }
    if msvc {
        build.define("_CRT_SECURE_NO_WARNINGS", None).define("UNICODE", None).define("WINVER", "0x0501");
    } else {
        build.define("HAVE_CONFIG_H", None);
    }
    for dir in &dirs {
        let mut files: Vec<PathBuf> = fs::read_dir(src.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "c"))
            .collect();
        files.sort();
        build.files(files);
    }
    build.compile("pff");
}
