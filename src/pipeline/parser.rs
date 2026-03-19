use std::fmt;
use std::error::Error;
use std::path::{Path, PathBuf};
use tree_sitter::{Parser, Tree, Node, Language};

/// Container for a parsed Dart file.
pub struct ParsedFile {
    /// Original file path.
    pub path: PathBuf,
    /// The tree-sitter AST.
    pub tree: Tree,
    /// The full source code.
    pub source: String,
}

/// Errors that can occur during parsing.
#[derive(Debug)]
pub enum ParseError {
    /// An IO error occurred while reading the file.
    Io(std::io::Error),
    /// A syntax error was detected in the Dart code.
    SyntaxError {
        path: PathBuf,
        line: usize,
        message: String,
    },
    /// Failed to initialize the tree-sitter parser or language.
    LanguageError,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(err) => write!(f, "IO error: {}", err),
            ParseError::SyntaxError { path, line, message } => {
                write!(f, "Parse error in {} at line {}: {}", path.display(), line, message)
            }
            ParseError::LanguageError => write!(f, "Failed to initialize Dart parser."),
        }
    }
}

impl Error for ParseError {}

/// Parses a Dart file into a ParsedFile struct.
pub fn parse_file(path: &Path) -> Result<ParsedFile, ParseError> {
    let source = std::fs::read_to_string(path).map_err(ParseError::Io)?;
    let mut parser = Parser::new();
    
    let language: Language = tree_sitter_dart::LANGUAGE.into();
    
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageError)?;

    let tree = parser
        .parse(&source, None)
        .ok_or(ParseError::LanguageError)?;

    if tree.root_node().has_error() {
        if let Some(error_node) = find_first_error(tree.root_node()) {
            let line = error_node.start_position().row + 1;
            return Err(ParseError::SyntaxError {
                path: path.to_path_buf(),
                line,
                message: "Syntax error".to_string(),
            });
        }
    }

    Ok(ParsedFile {
        path: path.to_path_buf(),
        tree,
        source,
    })
}

fn find_first_error(node: Node) -> Option<Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if child.has_error() {
                if let Some(err) = find_first_error(child) {
                    return Some(err);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_simple_dart() {
        let path = Path::new("testdata/simple.dart");
        let result = parse_file(path).unwrap();
        assert_eq!(result.path, path);
        assert!(!result.source.is_empty());
    }

    #[test]
    fn test_parse_error() {
        let path = Path::new("testdata/broken.dart");
        fs::write(path, "class Broken { oops").unwrap();
        
        let result = parse_file(path);
        assert!(matches!(result, Err(ParseError::SyntaxError { .. })));
        
        fs::remove_file(path).unwrap();
    }
}
