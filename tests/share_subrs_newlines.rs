//! The subroutine-share patch must apply to an LF checkout and to a normal
//! CRLF checkout, and it must still refuse a bad hunk.

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

#[path = "../patch.rs"]
mod patch;

fn sources() -> (String, String, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cff = fs::read_to_string(root.join("freetype2/src/cff/cffload.c")).unwrap();
    let header =
        fs::read_to_string(root.join("freetype2/include/freetype/internal/cfftypes.h")).unwrap();
    let patch_text = fs::read_to_string(root.join("patches/share-local-subrs.patch")).unwrap();
    assert!(!cff.contains('\r') && !header.contains('\r') && !patch_text.contains('\r'));
    (cff, header, patch_text)
}

fn crlf(text: &str) -> String {
    assert!(!text.contains('\r'));
    text.replace('\n', "\r\n")
}

fn stage(name: &str, cff: &str, header: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ft-share-hunk-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/cff")).unwrap();
    fs::create_dir_all(dir.join("include/freetype/internal")).unwrap();
    fs::write(dir.join("src/cff/cffload.c"), cff).unwrap();
    fs::write(dir.join("include/freetype/internal/cfftypes.h"), header).unwrap();
    dir
}

fn apply(dir: &Path, patch_text: &str) {
    patch::apply_unified(dir, patch_text);
}

fn patched(dir: &Path) -> (String, String) {
    let cff = fs::read_to_string(dir.join("src/cff/cffload.c")).unwrap();
    let header = fs::read_to_string(dir.join("include/freetype/internal/cfftypes.h")).unwrap();
    (cff, header)
}

fn assert_shared(cff: &str, header: &str) {
    assert_eq!(header.matches("FT_Bool  local_subrs_shared;").count(), 1);
    assert_eq!(cff.matches("subfont->local_subrs_shared = 1;").count(), 1);
    assert_eq!(
        cff.matches("if ( !subfont->local_subrs_shared )").count(),
        1
    );
    assert!(!cff.contains("FT_STREAM_SEEK( base_offset + top->private_offset +"));
    assert!(cff.contains("FT_STREAM_SEEK( subr_pos )"));
    assert!(!cff.contains('\r') && !header.contains('\r'));
}

fn panic_text(dir: &Path, patch_text: &str) -> String {
    let result = catch_unwind(AssertUnwindSafe(|| apply(dir, patch_text)));
    let payload = result.expect_err("hunk application must fail closed");
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        panic!("panic payload was not text");
    }
}

#[test]
fn lf_and_crlf_checkouts_apply_the_same_patch() {
    let (cff, header, patch_text) = sources();
    let lf = stage("lf", &cff, &header);
    apply(&lf, &patch_text);
    let (lf_cff, lf_header) = patched(&lf);
    assert_shared(&lf_cff, &lf_header);

    let crlf_dir = stage("crlf", &crlf(&cff), &crlf(&header));
    apply(&crlf_dir, &crlf(&patch_text));
    let (crlf_cff, crlf_header) = patched(&crlf_dir);
    assert_eq!(crlf_cff, lf_cff);
    assert_eq!(crlf_header, lf_header);

    let mixed_patch = stage("crlf-src-lf-patch", &crlf(&cff), &crlf(&header));
    apply(&mixed_patch, &patch_text);
    let (mixed_cff, mixed_header) = patched(&mixed_patch);
    assert_eq!(mixed_cff, lf_cff);
    assert_eq!(mixed_header, lf_header);
}

#[test]
fn an_unnormalized_crlf_checkout_contains_no_lf_hunk() {
    let (cff, header, patch_text) = sources();
    let crlf_cff = crlf(&cff);
    let crlf_header = crlf(&header);
    for line in patch_text.lines() {
        if let Some(body) = line.strip_prefix('-').or_else(|| line.strip_prefix(' ')) {
            if body.len() < 24 {
                continue;
            }
            let lf_needle = format!("{body}\n");
            let lf_hits = cff.matches(&lf_needle).count() + header.matches(&lf_needle).count();
            if lf_hits == 0 {
                continue;
            }
            let crlf_hits =
                crlf_cff.matches(&lf_needle).count() + crlf_header.matches(&lf_needle).count();
            assert_eq!(
                crlf_hits, 0,
                "LF needle still matched a CRLF checkout: {body}"
            );
        }
    }
}

#[test]
fn a_missing_hunk_matches_zero_times_and_does_not_write() {
    let (mut cff, header, patch_text) = sources();
    let marker = "error = cff_index_init( &subfont->local_subrs_index, stream, 1, cff2 );";
    assert_eq!(cff.matches(marker).count(), 1);
    cff = cff.replacen(marker, "error = 0;", 1);
    let dir = stage("missing", &cff, &header);
    let text = panic_text(&dir, &patch_text);
    assert!(
        text.contains("subroutine-share hunk matched 0 times"),
        "{}",
        text
    );
    let (out, header_out) = patched(&dir);
    assert!(!out.contains("local_subrs_shared"));
    assert!(!header_out.contains("local_subrs_shared"));
}

#[test]
fn a_duplicated_hunk_matches_twice() {
    let (cff, header, patch_text) = sources();
    let dir = stage("duplicate", &format!("{}{}", cff, cff), &header);
    let text = panic_text(&dir, &patch_text);
    assert!(
        text.contains("subroutine-share hunk matched 2 times"),
        "{}",
        text
    );
    let (out, header_out) = patched(&dir);
    assert!(!out.contains("local_subrs_shared"));
    assert!(!header_out.contains("local_subrs_shared"));
}

#[test]
fn a_malformed_hunk_line_fails_closed() {
    let (cff, header, patch_text) = sources();
    let broken = patch_text.replacen(
        "@@ -328,6 +328,11 @@",
        "@@ -328,6 +328,11 @@\nnot a hunk line",
        1,
    );
    let dir = stage("malformed", &cff, &header);
    let text = panic_text(&dir, &broken);
    assert!(
        text.contains("subroutine-share hunk line is malformed"),
        "{}",
        text
    );
    let (_, header_out) = patched(&dir);
    assert!(!header_out.contains("local_subrs_shared"));
}

#[test]
fn one_changed_character_is_not_the_hunk() {
    let (cff, header, patch_text) = sources();
    let shifted = cff.replacen(
        "FT_FREE( subfont->local_subrs );",
        "FT_FREE( subfont->local_subrs ); ",
        1,
    );
    let dir = stage("shifted", &shifted, &header);
    let text = panic_text(&dir, &patch_text);
    assert!(
        text.contains("subroutine-share hunk matched 0 times"),
        "{}",
        text
    );
    let (out, header_out) = patched(&dir);
    assert!(!out.contains("local_subrs_shared"));
    assert!(!header_out.contains("local_subrs_shared"));
}

#[test]
fn mixed_and_bare_cr_checkouts_fail_closed() {
    let (cff, header, patch_text) = sources();
    let mixed = cff.replacen('\n', "\r\n", 1);
    assert!(mixed.contains("\r\n") && mixed.contains('\n'));
    let dir = stage("mixed", &mixed, &header);
    let text = panic_text(&dir, &patch_text);
    assert!(text.contains("not uniform LF or CRLF"), "{}", text);
    let (out, header_out) = patched(&dir);
    assert!(!out.contains("local_subrs_shared"));
    assert!(!header_out.contains("local_subrs_shared"));

    let bare = cff.replacen("base_offset", "base_\roffset", 1);
    let bare_dir = stage("bare-cr", &bare, &header);
    let bare_text = panic_text(&bare_dir, &patch_text);
    assert!(
        bare_text.contains("not uniform LF or CRLF"),
        "{}",
        bare_text
    );
}
