use clap::Parser;
use crate::cli::{Cli, Command};
use std::path::PathBuf;
use dashmap::DashMap;

mod cli;
mod ir;
mod registry;
mod writer;
mod watcher;
mod pipeline;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cache = DashMap::new();

    match cli.command {
        Command::Build { verbose } => build(verbose, &cache),
        Command::Watch => watch(&cache),
        Command::Clean => clean(),
    }
}

fn build(verbose: bool, cache: &DashMap<PathBuf, u64>) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;
    
    if !root.join("lib").exists() {
        eprintln!("  Error: Cannot find 'lib/' directory.");
        eprintln!("  Run this command from your Flutter project root.");
        std::process::exit(1);
    }

    println!("  Scanning...");
    let result = pipeline::run(&root, verbose, cache)?;
    
    let total_processed = result.files_generated + result.files_skipped;
    if total_processed == 0 && result.errors.is_empty() {
        println!("  Scanning...  0 files with @Data() found.\n");
        println!("  Nothing to generate. Add @Data() to a class and re-run.");
        return Ok(());
    }

    println!("  Generating...\n");
    
    let mut show_count = 0;
    for (path, _) in &result.generated_content {
        if show_count < 3 {
            println!("  ✓ {}", path.file_name().unwrap().to_str().unwrap());
            show_count += 1;
        }
    }
    
    if result.generated_content.len() > 3 {
        println!("  ...  ({} more)", result.generated_content.len() - 3);
    }

    for (path, err) in &result.errors {
        println!("  ✗ {}  →  {}", path.file_name().unwrap().to_str().unwrap(), err);
    }

    if !result.errors.is_empty() {
        println!("\n  Done with errors. {} ok · {} failed · {}ms", 
            result.files_generated + result.files_skipped, result.files_failed, result.duration_ms);
        if !verbose {
            println!("  Run with --verbose to see full error details.");
        }
        std::process::exit(1);
    } else {
        let total = result.files_generated + result.files_skipped;
        println!("\n  Done. {} files · {}ms", total, result.duration_ms);
        let est_ms = total * 240;
        let speedup = est_ms as f64 / (result.duration_ms as f64).max(1.0);
        println!("  (build_runner est. ~{}s · {:.0}x faster)", est_ms / 1000, speedup);
    }

    Ok(())
}

fn watch(cache: &DashMap<PathBuf, u64>) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;
    if !root.join("lib").exists() {
        eprintln!("  Error: Cannot find 'lib/' directory.");
        eprintln!("  Run this command from your Flutter project root.");
        std::process::exit(1);
    }
    watcher::watch(&root, cache)?;
    Ok(())
}

fn clean() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir()?;
    let lib_dir = root.join("lib");
    if !lib_dir.exists() {
        println!("  Nothing to clean.");
        return Ok(());
    }

    let mut count = 0;
    for entry in walkdir::WalkDir::new(lib_dir) {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(|s| s.ends_with(".g.dart")) {
            std::fs::remove_file(entry.path())?;
            count += 1;
        }
    }

    if count > 0 {
        println!("  Deleted {} .g.dart files.", count);
    } else {
        println!("  Nothing to clean.");
    }
    Ok(())
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::Path;
    use std::fs;

    #[test]
    fn test_full_pipeline_on_testdata() {
        let root = Path::new("testdata/integration_test");
        let lib = root.join("lib");
        if root.exists() {
            fs::remove_dir_all(root).unwrap();
        }
        fs::create_dir_all(&lib).unwrap();
        
        fs::copy("testdata/simple.dart", lib.join("simple.dart")).unwrap();
        fs::copy("testdata/nested.dart", lib.join("nested.dart")).unwrap();
        fs::copy("testdata/generic.dart", lib.join("generic.dart")).unwrap();
        
        let cache = DashMap::new();
        let result = pipeline::run(root, false, &cache).unwrap();
        
        assert_eq!(result.files_generated, 3);
        assert_eq!(result.errors.len(), 0);
        
        // Run again, should be skipped
        let result2 = pipeline::run(root, false, &cache).unwrap();
        assert_eq!(result2.files_generated, 0);
        assert_eq!(result2.files_skipped, 3);
        
        fs::remove_dir_all(root).unwrap();
    }
}
