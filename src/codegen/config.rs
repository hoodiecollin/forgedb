//! Code generation configuration
//!
//! Provides configurable options for code generation including API paths,
//! SDK package metadata, output paths, and behavior flags.

use serde::{Deserialize, Serialize};

/// Configuration for code generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenConfig {
    /// Base path for API routes (e.g., "/api")
    pub api_base: String,
    
    /// Router prefix for generated routes (e.g., "/api")
    pub router_prefix: String,
    
    /// SDK package metadata
    pub sdk_package: PackageMeta,
    
    /// Output paths configuration
    pub paths: OutputPaths,
    
    /// Default behavior for soft delete
    pub soft_delete_default: bool,
    
    /// Pluralization mode
    pub pluralization: PluralizationMode,
    
    /// TypeScript target configuration
    pub ts_target: TsTarget,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        CodegenConfig {
            api_base: "/api".to_string(),
            router_prefix: "/api".to_string(),
            sdk_package: PackageMeta::default(),
            paths: OutputPaths::default(),
            soft_delete_default: false,
            pluralization: PluralizationMode::Inflector,
            ts_target: TsTarget::default(),
        }
    }
}

/// Package metadata for SDK generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    /// Package name
    pub name: String,
    
    /// Package version
    pub version: String,
    
    /// Package description
    pub description: String,
    
    /// Package author
    pub author: Option<String>,
    
    /// Package license
    pub license: Option<String>,
}

impl Default for PackageMeta {
    fn default() -> Self {
        PackageMeta {
            name: "forgedb-sdk".to_string(),
            version: "0.1.0".to_string(),
            description: "Auto-generated ForgeDB SDK".to_string(),
            author: None,
            license: Some("MIT".to_string()),
        }
    }
}

/// Output paths configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPaths {
    /// Path for generated Rust code
    pub rust: String,
    
    /// Path for TypeScript SDK
    pub typescript: String,
    
    /// Path for OpenAPI specification
    pub openapi: String,
    
    /// Path for API routes
    pub api_routes: String,
    
    /// Path for component stubs
    pub components: String,
}

impl Default for OutputPaths {
    fn default() -> Self {
        OutputPaths {
            rust: "generated/rust".to_string(),
            typescript: "generated/typescript".to_string(),
            openapi: "generated/openapi".to_string(),
            api_routes: "generated/api".to_string(),
            components: "generated/components".to_string(),
        }
    }
}

/// Pluralization mode for naming
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluralizationMode {
    /// Use inflector crate for smart pluralization
    Inflector,
    
    /// Simple pluralization (just add 's')
    Simple,
    
    /// No pluralization
    None,
}

/// TypeScript target configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsTarget {
    /// ECMAScript target (e.g., "ES2020")
    pub es_version: String,
    
    /// Module system (e.g., "ESNext", "CommonJS")
    pub module_system: String,
    
    /// Whether to include source maps
    pub source_maps: bool,
    
    /// Whether to generate declaration files
    pub declarations: bool,
}

impl Default for TsTarget {
    fn default() -> Self {
        TsTarget {
            es_version: "ES2020".to_string(),
            module_system: "ESNext".to_string(),
            source_maps: true,
            declarations: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CodegenConfig::default();
        assert_eq!(config.api_base, "/api");
        assert_eq!(config.router_prefix, "/api");
        assert_eq!(config.sdk_package.name, "forgedb-sdk");
        assert!(!config.soft_delete_default);
        assert_eq!(config.pluralization, PluralizationMode::Inflector);
    }

    #[test]
    fn test_custom_config() {
        let config = CodegenConfig {
            api_base: "/v1".to_string(),
            pluralization: PluralizationMode::Simple,
            ..Default::default()
        };
        assert_eq!(config.api_base, "/v1");
        assert_eq!(config.pluralization, PluralizationMode::Simple);
    }

    #[test]
    fn test_package_meta_default() {
        let meta = PackageMeta::default();
        assert_eq!(meta.name, "forgedb-sdk");
        assert_eq!(meta.version, "0.1.0");
        assert_eq!(meta.license, Some("MIT".to_string()));
    }

    #[test]
    fn test_output_paths_default() {
        let paths = OutputPaths::default();
        assert_eq!(paths.rust, "generated/rust");
        assert_eq!(paths.typescript, "generated/typescript");
        assert_eq!(paths.openapi, "generated/openapi");
    }

    #[test]
    fn test_ts_target_default() {
        let target = TsTarget::default();
        assert_eq!(target.es_version, "ES2020");
        assert_eq!(target.module_system, "ESNext");
        assert!(target.source_maps);
        assert!(target.declarations);
    }
}
