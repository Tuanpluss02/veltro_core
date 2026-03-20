use crate::ir::ClassIR;
use std::collections::HashMap;

pub mod veltro_plugin;

/// A file produced by a plugin.
pub struct GeneratedFile {
    /// Absolute path to the output file (e.g. "/project/lib/models/user.g.dart").
    pub path: String,
    /// The class body content (no file header — the pipeline adds it).
    pub content: String,
}

/// The contract every plugin must implement.
pub trait VeltroPlugin: Send + Sync {
    /// Annotation name without @ or (), e.g. "Veltro", "SlangGen".
    fn annotation(&self) -> &str;

    /// Generate file content for one annotated class.
    /// `options` comes from build.yaml under this annotation's key.
    fn generate(
        &self,
        ir: &ClassIR,
        options: &HashMap<String, String>,
    ) -> Vec<GeneratedFile>;
}

/// Holds all registered plugins, matched by annotation name.
pub struct PluginRegistry {
    plugins: Vec<Box<dyn VeltroPlugin>>,
}

impl PluginRegistry {
    /// Creates a registry with all built-in plugins pre-registered.
    pub fn with_defaults() -> Self {
        Self {
            plugins: vec![Box::new(veltro_plugin::VeltroCodePlugin)],
        }
    }

    /// Returns all plugins that handle a given annotation name.
    pub fn plugins_for(&self, annotation: &str) -> Vec<&dyn VeltroPlugin> {
        self.plugins
            .iter()
            .filter(|p| p.annotation() == annotation)
            .map(|p| p.as_ref())
            .collect()
    }
}
