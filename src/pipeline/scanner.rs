use std::collections::HashSet;
use std::fmt;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Errors that can occur during the scanning process.
#[derive(Debug)]
pub enum ScanError {
    /// A glob pattern was invalid.
    Glob(String),
    /// An IO error occurred.
    Io(std::io::Error),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::Glob(msg) => write!(f, "Invalid glob pattern: {}", msg),
            ScanError::Io(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(err: std::io::Error) -> Self {
        ScanError::Io(err)
    }
}

/// Scans for .dart files matching the given glob patterns relative to root.
/// Excludes .g.dart files. Deduplicates results.
pub fn scan(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>, ScanError> {
    let mut seen = HashSet::new();
    let mut dart_files = Vec::new();

    for pattern in patterns {
        let full_pattern = root.join(pattern).to_string_lossy().to_string();
        let entries = glob::glob(&full_pattern)
            .map_err(|e| ScanError::Glob(e.to_string()))?;

        for path in entries.flatten() {
            let is_generated = path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".g.dart"));
            if !is_generated && seen.insert(path.clone()) {
                dart_files.push(path);
            }
        }
    }

    Ok(dart_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_empty_for_nonexistent_root() {
        let root = Path::new("non_existent_project_root_for_test");
        let patterns = vec!["lib/**/*.dart".to_string()];
        let result = scan(root, &patterns).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_dart_files() {
        let root = Path::new("testdata/scanner_test");
        let lib = root.join("lib");
        if root.exists() {
            fs::remove_dir_all(root).unwrap();
        }
        fs::create_dir_all(&lib).unwrap();

        fs::write(lib.join("a.dart"), "").unwrap();
        fs::write(lib.join("b.g.dart"), "").unwrap();
        fs::write(lib.join("c.txt"), "").unwrap();

        let sub = lib.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("d.dart"), "").unwrap();

        let patterns = vec!["lib/**/*.dart".to_string()];
        let mut files = scan(root, &patterns).unwrap();
        files.sort();

        assert_eq!(files.len(), 2);
        assert!(files[0].to_str().unwrap().contains("a.dart"));
        assert!(files[1].to_str().unwrap().contains("d.dart"));

        fs::remove_dir_all(root).unwrap();
    }
}
