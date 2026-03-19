use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration loaded from build.yaml (or defaults if absent).
#[derive(Debug)]
pub struct BuildConfig {
    /// Glob patterns for source files. Default: ["lib/**/*.dart"]
    pub include_patterns: Vec<String>,
    /// Output path rules: source regex pattern → output path template (uses {} as capture placeholder).
    #[allow(dead_code)]
    pub build_extensions: HashMap<String, String>,
    /// Per-plugin options keyed by annotation name (lowercase).
    pub plugin_options: HashMap<String, HashMap<String, String>>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            include_patterns: vec!["lib/**/*.dart".to_string()],
            build_extensions: HashMap::new(),
            plugin_options: HashMap::new(),
        }
    }
}

impl BuildConfig {
    /// Loads build.yaml from the project root. Returns defaults if absent or unparseable.
    pub fn load(root: &Path) -> Self {
        let path = root.join("build.yaml");
        if !path.exists() {
            return Self::default();
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        let yaml: serde_yaml::Value = match serde_yaml::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Self::default(),
        };

        let veltro = match yaml.get("veltro") {
            Some(v) => v,
            None => return Self::default(),
        };

        // parse generate_for
        let include_patterns = veltro
            .get("generate_for")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| vec!["lib/**/*.dart".to_string()]);

        // parse build_extensions
        let build_extensions = veltro
            .get("build_extensions")
            .and_then(|v| v.as_mapping())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let key = k.as_str()?.to_string();
                        let val = v.as_str()?.to_string();
                        Some((key, val))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // parse builders[].annotation + options
        let plugin_options = veltro
            .get("builders")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|builder| {
                        let annotation = builder.get("annotation")?.as_str()?.to_lowercase();
                        let options = builder
                            .get("options")
                            .and_then(|o| o.as_mapping())
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k, v)| {
                                        let key = k.as_str()?.to_string();
                                        let val = v.as_str()?.to_string();
                                        Some((key, val))
                                    })
                                    .collect::<HashMap<String, String>>()
                            })
                            .unwrap_or_default();
                        Some((annotation, options))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            include_patterns,
            build_extensions,
            plugin_options,
        }
    }

    /// Resolves the output path for a source file.
    #[allow(dead_code)]
    /// Checks build_extensions rules (using {} as a capture placeholder); falls back to .g.dart.
    pub fn resolve_output_path(&self, source: &Path) -> PathBuf {
        let source_str = source.to_str().unwrap_or("");
        for (pattern, replacement) in &self.build_extensions {
            let regex_pattern = pattern.replace("{}", "(.+)");
            if let Ok(re) = regex::Regex::new(&regex_pattern) {
                if let Some(caps) = re.captures(source_str) {
                    let captured = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let result = replacement.replace("{}", captured);
                    return PathBuf::from(result);
                }
            }
        }
        source.with_extension("g.dart")
    }

    /// Returns options for a given annotation name (case-insensitive key lookup).
    pub fn options_for(&self, annotation: &str) -> HashMap<String, String> {
        self.plugin_options
            .get(&annotation.to_lowercase())
            .cloned()
            .unwrap_or_default()
    }
}
