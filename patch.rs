//! Apply `patches/share-local-subrs.patch` to the FreeType copy that is compiled.
//!
//! Git with `core.autocrlf=true` checks the indexed LF sources out as CRLF.
//! The hunk text is still exact: a uniform CRLF checkout is matched as LF, and
//! a missing, duplicated, or malformed hunk still stops the build.

use std::path::{Path, PathBuf};

pub fn apply_unified(root: &Path, patch: &str) {
    let mut lines = patch.lines().peekable();
    let mut path: Option<PathBuf> = None;
    let mut file = String::new();
    // A rejected hunk must not leave an earlier file of this patch written.
    let mut ready: Vec<(PathBuf, String)> = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            if let Some(path) = path.take() {
                ready.push((path, std::mem::take(&mut file)));
            }
            path = Some(PathBuf::from(rest));
            let raw = std::fs::read_to_string(root.join(rest))
                .unwrap_or_else(|err| panic!("reading {}: {}", rest, err));
            file = uniform_lf(rest, raw);
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
            if hunk == "\\ No newline at end of file" {
                continue;
            }
            if let Some(body) = hunk.strip_prefix('+') {
                push_line(&mut new, body);
            } else if let Some(body) = hunk.strip_prefix('-') {
                push_line(&mut old, body);
            } else if let Some(body) = hunk.strip_prefix(' ') {
                push_line(&mut old, body);
                push_line(&mut new, body);
            } else {
                panic!("subroutine-share hunk line is malformed: {:?}", hunk);
            }
        }
        let found = file.matches(&old).count();
        let where_ = path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        assert_eq!(
            found, 1,
            "subroutine-share hunk matched {} times in {}",
            found, where_
        );
        file = file.replacen(&old, &new, 1);
    }
    if let Some(path) = path {
        ready.push((path, file));
    }
    for (path, file) in ready {
        std::fs::write(root.join(path), file).unwrap();
    }
}

fn push_line(buf: &mut String, body: &str) {
    buf.push_str(body);
    buf.push('\n');
}

/// LF stays LF. A checkout whose every break is CRLF becomes LF. A bare CR or
/// a mix of LF and CRLF is not a normal checkout and does not match.
fn uniform_lf(name: &str, raw: String) -> String {
    let bytes = raw.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                saw_crlf = true;
                index += 2;
                continue;
            }
            panic!(
                "subroutine-share checkout of {} is not uniform LF or CRLF",
                name
            );
        }
        if bytes[index] == b'\n' {
            saw_lf = true;
        }
        index += 1;
    }
    if saw_lf && saw_crlf {
        panic!(
            "subroutine-share checkout of {} is not uniform LF or CRLF",
            name
        );
    }
    if saw_crlf {
        raw.replace("\r\n", "\n")
    } else {
        raw
    }
}
