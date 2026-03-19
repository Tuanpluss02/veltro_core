use std::path::{Path, PathBuf};
use dashmap::DashMap;
use xxhash_rust::xxh3::xxh3_64;
use std::io;

/// Result of a file write operation.
pub enum WriteResult {
    /// File was written to disk.
    Written,
    /// File was skipped because content hasn't changed.
    Skipped,
    /// An error occurred during the write.
    Error(io::Error),
}

/// Writes content to a file only if the content hash has changed.
pub fn write_if_changed(
    path: &Path,
    content: &str,
    cache: &DashMap<PathBuf, u64>,
) -> WriteResult {
    let new_hash = xxh3_64(content.as_bytes());
    
    if let Some(old_hash) = cache.get(path) {
        if *old_hash == new_hash {
            return WriteResult::Skipped;
        }
    }
    
    match std::fs::write(path, content) {
        Ok(_) => {
            cache.insert(path.to_path_buf(), new_hash);
            WriteResult::Written
        }
        Err(e) => WriteResult::Error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_write_if_changed() {
        let path = Path::new("testdata/writer_test.dart");
        let cache = DashMap::new();
        let content = "test content";
        
        // 1. First write
        let res = write_if_changed(path, content, &cache);
        assert!(matches!(res, WriteResult::Written));
        assert!(path.exists());
        
        // 2. Second write (same content)
        let res = write_if_changed(path, content, &cache);
        assert!(matches!(res, WriteResult::Skipped));
        
        // 3. Third write (different content)
        let res = write_if_changed(path, "new content", &cache);
        assert!(matches!(res, WriteResult::Written));
        
        fs::remove_file(path).unwrap();
    }
}
