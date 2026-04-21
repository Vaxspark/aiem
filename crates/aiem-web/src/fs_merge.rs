//! 3-way smart merge for skill updates (shared with GUI).

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Smart merge using 3-way comparison:
/// - current == original → overwrite with new version
/// - current != original → user modified → skip
/// Returns list of skipped file relative paths (forward slashes).
pub fn smart_merge(
    src: &Path,
    dst: &Path,
    original_hashes: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut skipped = Vec::new();
    let walker = walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if target.exists() {
            let current = std::fs::read(&target).unwrap_or_default();
            let current_hash = hex::encode(Sha256::digest(&current));
            if let Some(orig_hash) = original_hashes.get(&rel_str) {
                if &current_hash != orig_hash {
                    skipped.push(rel_str);
                    continue;
                }
            }
        }
        let _ = std::fs::copy(entry.path(), &target);
    }
    skipped
}

pub fn hash_files(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let walker = walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok());
    for entry in walker {
        if !entry.file_type().is_file() { continue; }
        let rel = match entry.path().strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if let Ok(bytes) = std::fs::read(entry.path()) {
            map.insert(rel, hex::encode(Sha256::digest(&bytes)));
        }
    }
    map
}
