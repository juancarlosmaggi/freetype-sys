use std::env;
use std::path::{Path, PathBuf};

fn add_sources(build: &mut cc::Build, root: &str, files: &[&str]) {
    let root = std::path::Path::new(root);
    build.files(files.iter().map(|src| {
        let mut p = root.join(src);
        p.set_extension("c");
        p
    }));

    build.include(root);
}

/// Copy the CFF driver and `cfftypes.h` and apply
/// `patches/share-local-subrs.patch`. Several Font DICTs often name one
/// local subroutine INDEX. FreeType otherwise allocates that INDEX's offset
/// table and pointer table once per DICT. The patch loads them once. The
/// copy is what gets compiled, so the submodule sources stay as published.
fn patched_cff() -> PathBuf {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("ft-share");
    let cff = out.join("src/cff");
    let header = out.join("include/freetype/internal");
    std::fs::create_dir_all(&cff).unwrap();
    std::fs::create_dir_all(&header).unwrap();
    for entry in std::fs::read_dir("freetype2/src/cff").unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            std::fs::copy(entry.path(), cff.join(entry.file_name())).unwrap();
        }
    }
    std::fs::copy(
        "freetype2/include/freetype/internal/cfftypes.h",
        header.join("cfftypes.h"),
    )
    .unwrap();
    let patch = std::fs::read_to_string("patches/share-local-subrs.patch").unwrap();
    apply_unified(&out, &patch);
    println!("cargo:rerun-if-changed=patches/share-local-subrs.patch");
    println!("cargo:rerun-if-changed=freetype2/src/cff/cffload.c");
    println!("cargo:rerun-if-changed=freetype2/include/freetype/internal/cfftypes.h");
    out
}

fn apply_unified(root: &Path, patch: &str) {
    let mut lines = patch.lines().peekable();
    let mut path: Option<PathBuf> = None;
    let mut file = String::new();
    let finish = |path: &Option<PathBuf>, file: &str| {
        if let Some(path) = path {
            std::fs::write(root.join(path), file).unwrap();
        }
    };
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            finish(&path, &file);
            path = Some(PathBuf::from(rest));
            file = std::fs::read_to_string(root.join(rest))
                .unwrap_or_else(|err| panic!("reading {}: {}", rest, err));
            continue;
        }
        if !line.starts_with("@@") {
            continue;
        }
        let mut old = String::new();
        let mut new = String::new();
        while let Some(next) = lines.peek() {
            if next.starts_with("@@") || next.starts_with("diff ") || next.starts_with("+++ ") {
                break;
            }
            let hunk = lines.next().unwrap();
            if hunk.starts_with('\\') {
                continue;
            }
            if let Some(body) = hunk.strip_prefix('+') {
                new.push_str(body);
                new.push('\n');
            } else if let Some(body) = hunk.strip_prefix('-') {
                old.push_str(body);
                old.push('\n');
            } else if let Some(body) = hunk.strip_prefix(' ') {
                old.push_str(body);
                old.push('\n');
                new.push_str(body);
                new.push('\n');
            }
        }
        let found = file.matches(&old).count();
        assert_eq!(
            found, 1,
            "subroutine-share hunk matched {} times",
            found
        );
        file = file.replacen(&old, &new, 1);
    }
    finish(&path, &file);
}

fn main() {
    if !cfg!(feature = "bundled") {
        let target = env::var("TARGET").unwrap();
        if !target.contains("android") && !target.contains("ohos") {
            pkg_config::Config::new()
                .atleast_version("24.3.18")
                .probe("freetype2")
                .unwrap();
        }
        return;
    }

    // From libpng/png.h:
    // "If pnglibconf.h is missing, you can copy scripts/pnglibconf.h.prebuilt to pnglibconf.h"
    std::fs::copy(
        "libpng/scripts/pnglibconf.h.prebuilt",
        "libpng/pnglibconf.h",
    )
    .unwrap();

    let mut build = cc::Build::new();
    let patched = patched_cff();

    build
        .warnings(false)
        .include(".")
        .include(patched.join("include"))
        .include("freetype2/include")
        .include("libpng")
        .define("FT2_BUILD_LIBRARY", None)
        .define("FT_CONFIG_OPTION_USE_PNG", None);

    add_sources(
        &mut build,
        "freetype2/src",
        &[
            "autofit/autofit",
            "base/ftbase",
            "base/ftbbox",
            "base/ftbdf",
            "base/ftbitmap",
            "base/ftcid",
            "base/ftdebug",
            "base/ftfstype",
            "base/ftgasp",
            "base/ftglyph",
            "base/ftgxval",
            "base/ftinit",
            "base/ftmm",
            "base/ftotval",
            "base/ftpatent",
            "base/ftpfr",
            "base/ftstroke",
            "base/ftsynth",
            "base/ftsystem",
            "base/fttype1",
            "base/ftwinfnt",
            "bdf/bdf",
            "bzip2/ftbzip2",
            "cache/ftcache",
            "cid/type1cid",
            "gzip/ftgzip",
            "lzw/ftlzw",
            "pcf/pcf",
            "pfr/pfr",
            "psaux/psaux",
            "pshinter/pshinter",
            "psnames/psnames",
            "raster/raster",
            "sdf/sdf",
            "svg/svg",
            "sfnt/sfnt",
            "smooth/smooth",
            "truetype/truetype",
            "type1/type1",
            "type42/type42",
            "winfonts/winfnt",
        ],
    );
    build.file(patched.join("src/cff/cff.c"));

    build.compile("freetype2");

    println!("cargo:rustc-link-lib=z");

    let mut build = cc::Build::new();
    build.include("libpng").include("libz-sys/src/zlib");
    build
        .file("libpng/png.c")
        .file("libpng/pngerror.c")
        .file("libpng/pngget.c")
        .file("libpng/pngmem.c")
        .file("libpng/pngpread.c")
        .file("libpng/pngread.c")
        .file("libpng/pngrio.c")
        .file("libpng/pngrtran.c")
        .file("libpng/pngrutil.c")
        .file("libpng/pngset.c")
        .file("libpng/pngtrans.c")
        .file("libpng/pngwio.c")
        .file("libpng/pngwrite.c")
        .file("libpng/pngwtran.c")
        .file("libpng/pngwutil.c");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if arch == "arm" || arch == "aarch64" {
        build
            .file("libpng/arm/arm_init.c")
            .file("libpng/arm/filter_neon_intrinsics.c")
            .file("libpng/arm/palette_neon_intrinsics.c");
        // MSVC cannot assemble libpng's GAS filter_neon.S — use C intrinsics only.
        if target_env == "msvc" {
            build.define("PNG_ARM_NEON_IMPLEMENTATION", Some("1"));
        } else {
            build.file("libpng/arm/filter_neon.S");
        }
    }

    build.compile("libpng.a");
}
