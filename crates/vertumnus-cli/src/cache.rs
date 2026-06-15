//! Incremental caching for the Vertumnus pipeline.
//!
//! Caches the IR and annotated IR on disk, keyed by content hash
//! of the crate's `src/` directory. This enables 10-50× faster re-wraps
//! during development by skipping re-inspection and re-mapping when
//! the source hasn't changed.
//!
//! Cache layout:
//! ```text
//! .cache/vertumnus/<crate_name>/
//!   ir.content_hash    # SHA256 of all source files
//!   ir.json            # cached IntermediateRepresentation
//!   annotated_ir.json  # cached AnnotatedIr
//! ```

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use vertumnus_inspector::IntermediateRepresentation;
use vertumnus_mapper::AnnotatedIr;

/// Cache configuration.
pub struct Cache {
    /// Path to the cache directory (e.g., `.cache/vertumnus/<crate_name>/`).
    cache_dir: PathBuf,
    /// The content hash of the source files at the time of caching.
    content_hash: String,
}

impl Cache {
    /// Create a new cache for the given crate.
    ///
    /// Computes the content hash and sets up the cache directory path.
    pub fn new(crate_dir: &Path) -> std::io::Result<Self> {
        let crate_name = crate_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown_crate");

        let content_hash = compute_source_hash(crate_dir)?;
        let cache_dir = crate_dir
            .join(".cache")
            .join("vertumnus")
            .join(crate_name);

        Ok(Self {
            cache_dir,
            content_hash,
        })
    }

    /// Try to load cached IR. Returns `None` if cache is missing or stale.
    pub fn load_ir(&self) -> Option<IntermediateRepresentation> {
        let hash_file = self.cache_dir.join("ir.content_hash");
        let ir_file = self.cache_dir.join("ir.json");

        // Check that hash file exists and matches
        let stored_hash = std::fs::read_to_string(&hash_file).ok()?;
        if stored_hash.trim() != self.content_hash {
            return None;
        }

        // Read and parse the cached IR
        let content = std::fs::read_to_string(&ir_file).ok()?;
        IntermediateRepresentation::from_json(&content).ok()
    }

    /// Try to load cached annotated IR. Returns `None` if cache is missing or stale.
    pub fn load_annotated_ir(&self) -> Option<AnnotatedIr> {
        let hash_file = self.cache_dir.join("ir.content_hash");
        let annotated_file = self.cache_dir.join("annotated_ir.json");

        // Check that hash file exists and matches
        let stored_hash = std::fs::read_to_string(&hash_file).ok()?;
        if stored_hash.trim() != self.content_hash {
            return None;
        }

        // Read and parse the cached annotated IR
        let content = std::fs::read_to_string(&annotated_file).ok()?;
        AnnotatedIr::from_json(&content).ok()
    }

    /// Save IR to cache.
    pub fn save_ir(&self, ir: &IntermediateRepresentation) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.cache_dir)?;

        // Write content hash
        std::fs::write(self.cache_dir.join("ir.content_hash"), &self.content_hash)?;

        // Write IR JSON
        let ir_json = ir.to_json_pretty().map_err(|e| {
            std::io::Error::other(format!("Serialization error: {e}"))
        })?;
        std::fs::write(self.cache_dir.join("ir.json"), ir_json)?;

        Ok(())
    }

    /// Save annotated IR to cache.
    pub fn save_annotated_ir(&self, annotated: &AnnotatedIr) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.cache_dir)?;

        // Write content hash (in case it wasn't written by save_ir)
        std::fs::write(self.cache_dir.join("ir.content_hash"), &self.content_hash)?;

        // Write annotated IR JSON
        let annotated_json = annotated.to_json_pretty().map_err(|e| {
            std::io::Error::other(format!("Serialization error: {e}"))
        })?;
        std::fs::write(self.cache_dir.join("annotated_ir.json"), annotated_json)?;

        Ok(())
    }
}

/// Compute a SHA256 hash of all `.rs` source files in the crate's `src/` directory.
///
/// Walks the `src/` directory recursively, reading every `.rs` file, and
/// computes a hash of their concatenated contents. If `src/` doesn't exist,
/// falls back to hashing files in the crate root.
fn compute_source_hash(crate_dir: &Path) -> std::io::Result<String> {
    let src_dir = crate_dir.join("src");
    let search_dir = if src_dir.is_dir() { src_dir } else { crate_dir.to_path_buf() };

    let mut hasher = Sha256::new();
    let mut files_hashed = 0u64;

    // Walk the directory and collect all .rs file paths, then sort them
    // to ensure deterministic ordering.
    let mut rs_files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&search_dir)
        .into_iter()
        .filter_entry(|e| {
            // Skip hidden directories (like .cache, .git)
            e.file_name()
                .to_str()
                .map(|s| !s.starts_with('.'))
                .unwrap_or(false)
        })
    {
        let entry = entry?;
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "rs" {
                    rs_files.push(entry.path().to_path_buf());
                }
            }
        }
    }

    // Sort files for deterministic hashing
    rs_files.sort();

    for file_path in &rs_files {
        let content = std::fs::read(file_path)?;
        hasher.update(&content);
        files_hashed += 1;
    }

    // Also hash Cargo.toml if it exists
    let cargo_toml = crate_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        let content = std::fs::read(&cargo_toml)?;
        hasher.update(&content);
    }

    // Include file count to differentiate empty-source changes
    hasher.update(files_hashed.to_le_bytes());

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_compute_source_hash_consistency() {
        let dir = std::env::temp_dir().join("vertumnus_test_hash_consistency");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn foo() -> i32 { 42 }").unwrap();

        let hash1 = compute_source_hash(&dir).unwrap();
        let hash2 = compute_source_hash(&dir).unwrap();
        assert_eq!(hash1, hash2, "Hash should be consistent");

        // Change a file
        fs::write(src.join("lib.rs"), "pub fn bar() -> i64 { 99 }").unwrap();
        let hash3 = compute_source_hash(&dir).unwrap();
        assert_ne!(hash1, hash3, "Hash should change when source changes");

        // Cleanup
        fs::remove_dir_all(&dir).unwrap_or(());
    }

    #[test]
    fn test_cache_roundtrip() {
        let dir = std::env::temp_dir().join("vertumnus_test_cache_roundtrip");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn foo() {}").unwrap();

        let cache = Cache::new(&dir).unwrap();

        // Save and load IR
        let ir = IntermediateRepresentation::new("test".to_string(), "1.0.0".to_string());
        cache.save_ir(&ir).unwrap();

        let loaded = cache.load_ir();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().crate_name, "test");

        // Save and load annotated IR
        let annotated = AnnotatedIr::new("test".to_string(), "1.0.0".to_string());
        cache.save_annotated_ir(&annotated).unwrap();

        let loaded_annotated = cache.load_annotated_ir();
        assert!(loaded_annotated.is_some());
        assert_eq!(loaded_annotated.unwrap().crate_name, "test");

        // Modify source — cache should be stale
        fs::write(src.join("lib.rs"), "pub fn bar() {}").unwrap();
        let cache2 = Cache::new(&dir).unwrap();
        assert!(cache2.load_ir().is_none(), "Cache should be stale after source change");
        assert!(cache2.load_annotated_ir().is_none(), "Cache should be stale after source change");

        // Cleanup
        fs::remove_dir_all(&dir).unwrap_or(());
    }

    #[test]
    fn test_cache_missing() {
        let dir = std::env::temp_dir().join(format!("vertumnus_cache_missing_{}", std::process::id()));
        // Directory doesn't actually exist — Cache::new still works (checks src dir existence)
        fs::create_dir_all(&dir).unwrap();
        let cache = Cache::new(&dir).unwrap();
        assert!(cache.load_ir().is_none());
        assert!(cache.load_annotated_ir().is_none());
        fs::remove_dir_all(&dir).unwrap_or(());
    }
}
