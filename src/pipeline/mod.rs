pub mod scanner;
pub mod parser;
pub mod analyzer;
pub mod resolver;
pub mod generator;

use std::fmt;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::collections::HashMap;
use rayon::prelude::*;
use crate::ir::DataClassIR;
use crate::registry::TypeRegistry;
use crate::writer::{self, WriteResult};
use dashmap::DashMap;

/// Result of a build run.
pub struct BuildResult {
    /// Number of files successfully generated.
    pub files_generated: usize,
    /// Number of files skipped (no changes).
    pub files_skipped: usize,
    /// Number of files that failed during the process.
    pub files_failed: usize,
    /// Total duration of the build in milliseconds.
    pub duration_ms: u128,
    /// List of errors encountered, with file path and error message.
    pub errors: Vec<(PathBuf, String)>,
    /// Generated content for each file (output_path, content).
    pub generated_content: Vec<(PathBuf, String)>,
}

/// Errors that can occur in the pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// Error during scanning.
    Scanner(scanner::ScanError),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::Scanner(e) => write!(f, "Scanner error: {}", e),
        }
    }
}

impl Error for PipelineError {}

/// Runs the full build pipeline.
pub fn run(root: &Path, _verbose: bool, cache: &DashMap<PathBuf, u64>) -> Result<BuildResult, PipelineError> {
    let start = Instant::now();
    
    // 1. Scan lib/
    let files = scanner::scan(root).map_err(PipelineError::Scanner)?;
    
    // 2. Parse files in parallel
    let parse_results: Vec<_> = files.into_par_iter()
        .map(|p| (p.clone(), parser::parse_file(&p)))
        .collect();
    
    let mut parsed_files = Vec::new();
    let mut errors = Vec::new();
    
    for (path, res) in parse_results {
        match res {
            Ok(p) => parsed_files.push(p),
            Err(e) => errors.push((path, e.to_string())),
        }
    }

    // 3. Analyze parsed files in parallel
    let analyze_results: Vec<_> = parsed_files.par_iter()
        .map(|p| (p, analyzer::analyze(p)))
        .collect();
        
    let mut ir_list = Vec::new();
    for (p, res) in analyze_results {
        match res {
            Ok(irs) => ir_list.extend(irs),
            Err(e) => errors.push((p.path.clone(), e.to_string())),
        }
    }
    
    if ir_list.is_empty() {
        return Ok(BuildResult {
            files_generated: 0,
            files_skipped: 0,
            files_failed: errors.len(),
            duration_ms: start.elapsed().as_millis(),
            errors,
            generated_content: Vec::new(),
        });
    }

    // 4. Build TypeRegistry (Pass 1)
    let registry = TypeRegistry::build(&ir_list, &parsed_files);
    
    // 5. Resolve IR in parallel (Pass 2)
    let resolved_ir: Vec<DataClassIR> = ir_list.into_par_iter()
        .map(|ir| resolver::resolve(ir, &registry))
        .collect();
        
    // 6. Generate code for each source file
    let mut grouped_ir: HashMap<PathBuf, Vec<DataClassIR>> = HashMap::new();
    for ir in resolved_ir {
        grouped_ir.entry(ir.source_file.clone()).or_default().push(ir);
    }

    let mut generated_content = Vec::new();
    let mut files_generated = 0;
    let mut files_skipped = 0;

    for (source_path, irs) in grouped_ir {
        let content = generator::generate(&irs);
        let mut output_path = source_path.clone();
        output_path.set_extension("g.dart");
        
        match writer::write_if_changed(&output_path, &content, cache) {
            WriteResult::Written => {
                files_generated += 1;
                generated_content.push((output_path, content));
            }
            WriteResult::Skipped => {
                files_skipped += 1;
            }
            WriteResult::Error(e) => {
                errors.push((output_path, e.to_string()));
            }
        }
    }
    
    Ok(BuildResult {
        files_generated,
        files_skipped,
        files_failed: errors.len(),
        duration_ms: start.elapsed().as_millis(),
        errors,
        generated_content,
    })
}
