//! Streaming tar archive reader for directory transfers.
//!
//! Directory packaging uses the synchronous `tar` crate on a dedicated thread
//! and forwards archive chunks into the encrypted async send pipeline.
//!
//! Extraction also runs on a dedicated thread because archive parsing and
//! filesystem traversal are synchronous. The extractor validates paths before
//! unpacking entries and safely recreates hard links detected by the sender.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::EngineError;

// ---------------------------------------------------------------------------
// Synchronous tar generator (run inside compio::spawn_blocking)
// ---------------------------------------------------------------------------

/// Generates a POSIX ustar tar stream for `root_dir` and writes it into `out`.
///
/// Detects hard-linked files (by device + inode on Unix) and emits tar `Link`
/// entries for duplicate occurrences instead of duplicating the file content.
/// This avoids inflating the transfer when sending directories like Rust
/// `target/` that use hard links heavily for incremental compilation cache.
///
/// This function is entirely synchronous and must be called on a dedicated
/// thread when used from async code.
pub fn write_tar_sync(root_dir: &Path, out: &mut impl io::Write) -> Result<(), io::Error> {
    let mut builder = tar::Builder::new(out);
    builder.follow_symlinks(false);
    append_dir_dedup(&mut builder, root_dir)?;
    builder.finish()?;
    Ok(())
}

/// Walks `root_dir` top-down and appends entries to the tar builder, detecting
/// hard-linked files and emitting `Link` entries for duplicates.
fn append_dir_dedup<W: io::Write>(
    builder: &mut tar::Builder<W>,
    root_dir: &Path,
) -> Result<(), io::Error> {
    // (device, inode) → first relative path for hard-link detection.
    // On Unix this deduplicates hard-linked files; on other platforms the map
    // stays empty because nlink is always reported as 1, so every file is
    // treated as independent.
    let mut seen: HashMap<(u64, u64), PathBuf> = HashMap::new();

    for entry in walkdir::WalkDir::new(root_dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        let relative = path.strip_prefix(root_dir).unwrap_or(path);

        if entry.file_type().is_dir() {
            // The root directory itself is skipped (tar convention: entries
            // are relative to the root). walkdir emits the root with depth 0,
            // which means `relative` is either empty or "." after stripping.
            let is_root = relative.as_os_str().is_empty() || relative == Path::new(".");
            if !is_root {
                builder.append_dir(relative, path)?;
            }
        } else if entry.file_type().is_file() {
            // Try to detect hard links from metadata. If metadata fails
            // (e.g. TOCTOU race), treat the file as a first occurrence
            // and let append_path_with_name surface any real IO error.
            let mut is_dup = false;
            #[cfg_attr(not(unix), allow(unused_variables))]
            if let Ok(meta) = entry.metadata() {
                #[cfg(unix)]
                let nlink = {
                    use std::os::unix::fs::MetadataExt;
                    meta.nlink()
                };
                #[cfg(not(unix))]
                let nlink: u64 = 1;

                if nlink > 1 {
                    #[cfg(unix)]
                    let key = {
                        use std::os::unix::fs::MetadataExt;
                        (meta.dev(), meta.ino())
                    };
                    #[cfg(not(unix))]
                    let key = (0u64, 0u64);

                    if let Some(first) = seen.get(&key) {
                        // Duplicate inode — emit a hard-link entry pointing
                        // back to the first occurrence.
                        let link_target = relative_path_from_to(
                            relative.parent().unwrap_or(Path::new(".")),
                            first,
                        );
                        let mut header = tar::Header::new_gnu();
                        header.set_entry_type(tar::EntryType::Link);
                        header.set_size(0);
                        builder.append_link(&mut header, relative, link_target)?;
                        is_dup = true;
                    } else {
                        seen.insert(key, relative.to_path_buf());
                    }
                }
            }

            if !is_dup {
                // First occurrence — append as a regular file.
                builder.append_path_with_name(path, relative)?;
            }
        }
        // Symlinks are skipped (follow_symlinks(false)).
    }

    Ok(())
}

/// Computes the relative path from `from_dir` to `to_path`.
/// Both are expected to be relative to the archive root.
///
/// Example: `from_dir = "target/debug/b"`, `to_path = "target/debug/a/file"`
/// → `"../a/file"`.
fn relative_path_from_to(from_dir: &Path, to_path: &Path) -> PathBuf {
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = to_path.components().collect();

    let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for comp in &to[common..] {
        result.push(comp);
    }
    result
}

// ---------------------------------------------------------------------------
// Safe extraction
// ---------------------------------------------------------------------------

/// Extracts a tar stream read from `input` into `output_dir`.
///
/// Path-traversal protection: rejects any entry whose cleaned path starts
/// outside `output_dir` (i.e. `..` components or absolute paths).
/// Hard links are allowed with validated link targets; symlinks are rejected.
pub fn extract_tar_sync(input: impl io::Read, output_dir: &Path) -> Result<(), EngineError> {
    std::fs::create_dir_all(output_dir).map_err(EngineError::Io)?;
    let output_dir = std::fs::canonicalize(output_dir).map_err(EngineError::Io)?;

    let mut archive = tar::Archive::new(input);
    let mut pending_hard_links = Vec::new();
    for entry in archive.entries().map_err(EngineError::Io)? {
        let mut entry = entry.map_err(EngineError::Io)?;
        let entry_path = entry.path().map_err(EngineError::Io)?;
        let entry_type = entry.header().entry_type();

        if entry_type.is_symlink() {
            return Err(EngineError::PathTraversal);
        }

        // Validate: no absolute paths, no `..` traversal.
        if entry_path.is_absolute() {
            return Err(EngineError::PathTraversal);
        }
        for component in entry_path.components() {
            if matches!(component, Component::ParentDir) {
                return Err(EngineError::PathTraversal);
            }
        }

        let dest = output_dir.join(&entry_path);
        // Final check: dest must be under output_dir.
        if !dest.starts_with(&output_dir) {
            return Err(EngineError::PathTraversal);
        }

        if entry_type.is_hard_link() {
            // link_name() returns Option<Cow<Path>> — unwrap both layers.
            let link_target =
                entry.link_name().map_err(EngineError::Io)?.ok_or(EngineError::PathTraversal)?;

            // Validate the link target path.
            if link_target.is_absolute() {
                return Err(EngineError::PathTraversal);
            }
            // Resolve the target relative to the link entry's parent directory.
            let link_dest = if let Some(parent) = dest.parent() {
                parent.join(&link_target)
            } else {
                output_dir.join(&link_target)
            };
            // Lexically normalise and check containment.
            let normalized = normalize_path(&link_dest);
            if !normalized.starts_with(&output_dir) {
                return Err(EngineError::PathTraversal);
            }

            pending_hard_links.push((dest, normalized));
            continue;
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(EngineError::Io)?;
        }
        entry.unpack(&dest).map_err(EngineError::Io)?;
    }

    // Replay collected hard links after all regular entries have been
    // extracted, so an archive may safely reference a target that appears
    // later in the stream.
    for (dest, normalized) in pending_hard_links {
        if !normalized.is_file() {
            return Err(EngineError::PathTraversal);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(EngineError::Io)?;
        }
        std::fs::hard_link(&normalized, &dest).map_err(EngineError::Io)?;
    }

    Ok(())
}

/// Lexically normalises a path by resolving `..` and removing `.` components.
/// This is a pure string-level operation — it does not access the filesystem.
///
/// Tracks depth so that `..` components that would escape the implied root
/// are preserved as leading `..` (unlike `PathBuf::pop`, which is a no-op
/// on an empty path).
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let mut depth: i32 = 0;
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if depth > 0 {
                    result.pop();
                    depth -= 1;
                } else {
                    result.push("..");
                }
            },
            Component::CurDir => {},
            c => {
                result.push(c);
                depth += 1;
            },
        }
    }
    result
}

/// Estimates the total size of all files under `root_dir` by walking the tree.
///
/// On Unix, hard-linked files (same device + inode) are counted only once so
/// the estimate matches the actual bytes that will be sent over the wire.
/// Used to report an approximate total for the progress bar.
pub fn estimate_dir_size(root_dir: &Path) -> u64 {
    // On Unix, deduplicate hard-linked files by (device, inode).
    // On other platforms the set stays empty because nlink is always 1.
    #[cfg_attr(not(unix), allow(unused_mut, unused_variables))]
    let mut seen: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();

    walkdir::WalkDir::new(root_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.nlink() > 1 && !seen.insert((meta.dev(), meta.ino())) {
                    return None; // already counted this inode
                }
            }
            Some(meta.len())
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_output(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!("hayate-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn extract_tar_creates_output_root() {
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let bytes = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_path("nested/file.txt").unwrap();
            header.set_size(bytes.len() as u64);
            header.set_cksum();
            builder.append(&header, bytes.as_slice()).unwrap();
            builder.finish().unwrap();
        }

        let out = temp_output("extract-root");
        let result = extract_tar_sync(Cursor::new(archive), &out);
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(out.join("nested/file.txt")).unwrap(), "hello");

        fs::remove_dir_all(out).unwrap();
    }

    #[test]
    fn extract_tar_rejects_symlink_entries() {
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_path("link").unwrap();
            header.set_link_name("../outside").unwrap();
            header.set_size(0);
            header.set_cksum();
            builder.append(&header, Cursor::new(Vec::new())).unwrap();
            builder.finish().unwrap();
        }

        let out = temp_output("reject-link");
        let result = extract_tar_sync(Cursor::new(archive), &out);
        assert!(matches!(result, Err(EngineError::PathTraversal)));

        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn hard_link_roundtrip() {
        let src = temp_output("hardlink-src");
        let dst = temp_output("hardlink-dst");

        // Create a directory with a file and a hard link to it.
        let sub = src.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("original.txt"), b"same content").unwrap();
        fs::hard_link(sub.join("original.txt"), sub.join("link.txt")).unwrap();

        // Pack into a tar stream.
        let mut archive = Vec::new();
        write_tar_sync(&src, &mut archive).unwrap();

        // Unpack.
        extract_tar_sync(Cursor::new(&archive), &dst).unwrap();

        // Both entries should exist with the same content.
        let orig = dst.join("sub/original.txt");
        let link = dst.join("sub/link.txt");
        assert!(orig.is_file());
        assert!(link.is_file());
        assert_eq!(fs::read_to_string(&orig).unwrap(), "same content");
        assert_eq!(fs::read_to_string(&link).unwrap(), "same content");

        // On Unix, verify it's actually a hard link (same inode).
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let orig_meta = fs::metadata(&orig).unwrap();
            let link_meta = fs::metadata(&link).unwrap();
            assert_eq!(orig_meta.ino(), link_meta.ino());
            assert!(orig_meta.nlink() >= 2);
        }

        let _ = fs::remove_dir_all(src);
        let _ = fs::remove_dir_all(dst);
    }

    #[test]
    fn hard_link_rejects_path_traversal() {
        let out = temp_output("hardlink-reject");

        // Craft a tar with a hard-link entry whose target escapes the output.
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            // Regular file first so it exists.
            let mut header = tar::Header::new_gnu();
            header.set_path("innocent.txt").unwrap();
            header.set_size(5);
            header.set_cksum();
            builder.append(&header, Cursor::new(b"hello")).unwrap();

            // Hard link pointing outside via `..`.
            let mut link_header = tar::Header::new_gnu();
            link_header.set_entry_type(tar::EntryType::Link);
            link_header.set_path("escape").unwrap();
            link_header.set_link_name("../../../etc/passwd").unwrap();
            link_header.set_size(0);
            link_header.set_cksum();
            builder.append(&link_header, Cursor::new(Vec::new())).unwrap();
            builder.finish().unwrap();
        }

        let result = extract_tar_sync(Cursor::new(&archive), &out);
        assert!(matches!(result, Err(EngineError::PathTraversal)));

        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn estimate_dir_size_dedup_hard_links() {
        let dir = temp_output("estimate-dedup");

        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"hello world").unwrap(); // 11 bytes
        fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();
        fs::hard_link(dir.join("a.txt"), dir.join("c.txt")).unwrap();

        let total = estimate_dir_size(&dir);
        // On Unix, inode dedup counts 11 bytes once. On other platforms
        // inode detection is unavailable, so each hard link is counted
        // independently.
        #[cfg(unix)]
        assert_eq!(total, 11);
        #[cfg(not(unix))]
        assert_eq!(total, 33);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relative_path_from_to_same_dir() {
        let result = relative_path_from_to(
            Path::new("target/debug/incremental"),
            Path::new("target/debug/incremental/hayate-abc"),
        );
        assert_eq!(result, PathBuf::from("hayate-abc"));
    }

    #[test]
    fn relative_path_from_to_parent_dir() {
        let result =
            relative_path_from_to(Path::new("target/debug/x"), Path::new("target/debug/y/file"));
        assert_eq!(result, PathBuf::from("../y/file"));
    }

    #[test]
    fn relative_path_from_to_nested() {
        let result = relative_path_from_to(Path::new("a/b/c"), Path::new("a/d/e"));
        assert_eq!(result, PathBuf::from("../../d/e"));
    }

    #[test]
    fn normalize_path_removes_dot() {
        assert_eq!(normalize_path(Path::new("foo/./bar")), PathBuf::from("foo/bar"));
    }

    #[test]
    fn normalize_path_resolves_dotdot() {
        assert_eq!(normalize_path(Path::new("foo/bar/../baz")), PathBuf::from("foo/baz"));
    }

    #[test]
    fn normalize_path_does_not_escape_root() {
        assert_eq!(normalize_path(Path::new("foo/../../baz")), PathBuf::from("../baz"));
    }
}
