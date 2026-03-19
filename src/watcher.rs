use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::fmt;
use std::error::Error;
use notify::{Watcher, RecursiveMode, EventKind};
use crossbeam_channel::unbounded;
use dashmap::DashMap;
use crate::pipeline;

/// Errors that can occur in the watcher.
#[derive(Debug)]
pub enum WatchError {
    /// Error from the notify crate.
    Notify(notify::Error),
    /// An IO error.
    Io(std::io::Error),
}

impl fmt::Display for WatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WatchError::Notify(e) => write!(f, "Watcher error: {}", e),
            WatchError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl Error for WatchError {}

/// Continuously watches lib/ for changes and runs the pipeline.
pub fn watch(root: &Path, cache: &DashMap<PathBuf, u64>) -> Result<(), WatchError> {
    // Set up Ctrl+C handler
    ctrlc::set_handler(move || {
        println!("\n  ^C  Stopped.");
        std::process::exit(0);
    }).expect("Error setting Ctrl+C handler");

    println!("  Watching lib/ for changes...  (Ctrl+C to stop)");
    
    // Initial build
    let initial_res = pipeline::run(root, false, cache).map_err(|e| {
        WatchError::Io(std::io::Error::other(e.to_string()))
    })?;
    
    println!("  Initial build: {} files · {}ms", 
        initial_res.files_generated + initial_res.files_skipped, initial_res.duration_ms);

    let (tx, rx) = unbounded();
    
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    }).map_err(WatchError::Notify)?;

    watcher.watch(&root.join("lib"), RecursiveMode::Recursive).map_err(WatchError::Notify)?;

    loop {
        if let Ok(event) = rx.recv() {
            // Debounce: wait 100ms for more events
            std::thread::sleep(Duration::from_millis(100));
            let mut events = vec![event];
            while let Ok(e) = rx.try_recv() {
                events.push(e);
            }
            
            process_events(events, root, cache);
        }
    }
}

fn process_events(events: Vec<notify::Event>, root: &Path, cache: &DashMap<PathBuf, u64>) {
    let mut changed = false;
    let mut removed_paths = Vec::new();

    for event in events {
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any => {
                for path in event.paths {
                    if path.extension().and_then(|s| s.to_str()) == Some("dart") && 
                       !path.to_str().unwrap().ends_with(".g.dart") {
                        changed = true;
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if path.extension().and_then(|s| s.to_str()) == Some("dart") {
                        removed_paths.push(path);
                    }
                }
            }
            _ => {}
        }
    }

    for path in removed_paths {
        let mut g_path = path.clone();
        g_path.set_extension("g.dart");
        if g_path.exists() {
            let _ = std::fs::remove_file(&g_path);
            cache.remove(&g_path);
        }
    }

    if changed {
        let start = Instant::now();
        match pipeline::run(root, false, cache) {
            Ok(result) => {
                let now = chrono::Local::now().format("%H:%M:%S");
                // result.generated_content only contains files that were actually written (changed)
                for (path, _) in result.generated_content {
                    let source_name = path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .replace(".g.dart", ".dart");
                    println!("  [{}]  {} changed → {}  ({}ms)", 
                        now, source_name, path.file_name().and_then(|s| s.to_str()).unwrap_or(""), 
                        start.elapsed().as_millis());
                }
            }
            Err(e) => {
                let now = chrono::Local::now().format("%H:%M:%S");
                println!("  [{}]  FAILED  {}", now, e);
            }
        }
    }
}
