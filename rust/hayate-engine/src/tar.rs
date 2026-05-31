//! Streaming tar archive reader for directory transfers.
//!
//! `TarReader` walks a directory tree using `std::fs` (synchronous, done on a
//! compio blocking thread to avoid blocking the async executor) and yields
//! raw tar bytes through a `std::io::Read` adapter.  The caller feeds those
//! bytes into the encrypted send pipeline.
//!
//! For extraction, `extract_tar` reads from an `AsyncRead` and writes files
//! via `compio::fs::OpenOptions` so every file write is a true async
//! completion operation.

use std::{
    io,
    path::{Component, Path},
};

use crate::EngineError;

// ---------------------------------------------------------------------------
// Synchronous tar generator (run inside compio::spawn_blocking)
// ---------------------------------------------------------------------------

/// Generates a POSIX ustar tar stream for `root_dir` and writes it into `out`.
/// This function is entirely synchronous and must be called inside
/// `compio::runtime::spawn_blocking` or on a dedicated std thread.
pub fn write_tar_sync(root_dir: &Path, out: &mut impl io::Write) -> Result<u64, io::Error> {
    let mut builder = tar::Builder::new(out);
    builder.follow_symlinks(false);
    builder.append_dir_all(".", root_dir)?;
    builder.finish()?;
    // Return 0 — actual byte count not needed; caller tracks progress via
    // the write callbacks.
    Ok(0)
}

// ---------------------------------------------------------------------------
// Safe extraction
// ---------------------------------------------------------------------------

/// Extracts a tar stream read from `input` into `output_dir`.
///
/// Path-traversal protection: rejects any entry whose cleaned path starts
/// outside `output_dir` (i.e. `..` components or absolute paths).
pub fn extract_tar_sync(input: impl io::Read, output_dir: &Path) -> Result<(), EngineError> {
    let mut archive = tar::Archive::new(input);
    for entry in archive.entries().map_err(EngineError::Io)? {
        let mut entry = entry.map_err(EngineError::Io)?;
        let entry_path = entry.path().map_err(EngineError::Io)?;

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
        if !dest.starts_with(output_dir) {
            return Err(EngineError::PathTraversal);
        }
        entry.unpack(&dest).map_err(EngineError::Io)?;
    }
    Ok(())
}

/// Estimates the total size of all files under `root_dir` by walking the tree.
/// Used to report an approximate total for the progress bar.
pub fn estimate_dir_size(root_dir: &Path) -> u64 {
    walkdir::WalkDir::new(root_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
