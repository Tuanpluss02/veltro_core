use std::fmt;
use std::error::Error;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use rayon::prelude::*;

/// Errors that can occur during the scanning process.
#[derive(Debug)]
pub enum ScanError {
    /// lib/ directory was not found.
    LibNotFound,
    /// An IO error occurred.
    Io(std::io::Error),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::LibNotFound => write!(f, "Cannot find 'lib/' directory."),
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

/// Scans lib/ recursively for .dart files, excluding .g.dart files.
pub fn scan(root: &Path) -> Result<Vec<PathBuf>, ScanError> {
    let lib_dir = root.join("lib");
    if !lib_dir.exists() || !lib_dir.is_dir() {
        return Err(ScanError::LibNotFound);
    }

    let entries: Vec<PathBuf> = WalkDir::new(&lib_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    let dart_files: Vec<PathBuf> = entries
        .into_par_iter()
        .filter(|p| {
            let is_dart = p.extension().and_then(|s| s.to_str()) == Some("dart");
            let is_generated = p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".g.dart"));
            is_dart && !is_generated
        })
        .collect();

    Ok(dart_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_lib_not_found() {
        let root = Path::new("non_existent_project_root_for_test");
        let result = scan(root);
        assert!(matches!(result, Err(ScanError::LibNotFound)));
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

        let mut files = scan(root).unwrap();
        files.sort();
        
        assert_eq!(files.len(), 2);
        assert!(files[0].to_str().unwrap().contains("a.dart"));
        assert!(files[1].to_str().unwrap().contains("d.dart"));
        
        fs::remove_dir_all(root).unwrap();
    }
}
