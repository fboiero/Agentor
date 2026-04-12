//! LangChain compatibility layer.
//!
//! Provides helpers to use LangChain tools, agents, and chains from within
//! Argentor agents. The adapter is a thin façade over
//! [`crate::dynamic_load::PythonToolSkill`] that encodes the specific module
//! paths and naming conventions used by the LangChain Python project.
//!
//! # Design
//!
//! LangChain evolves rapidly and classes move between packages (`langchain`,
//! `langchain_core`, `langchain_community`). The adapter tries the canonical
//! paths in order:
//!
//! 1. `langchain.tools.<ClassName>`
//! 2. `langchain_community.tools.<ClassName>`
//! 3. `langchain_core.tools.<ClassName>`
//!
//! The first successful import wins. This means upgrading LangChain versions
//! rarely breaks tool loading.
//!
//! All calls into the Python interpreter are gated behind `#[cfg(not(test))]`
//! so that unit tests only cover the configuration/plumbing surface without
//! requiring LangChain or Python to be installed.

#![allow(dead_code)]

use crate::dynamic_load::{PythonToolConfig, PythonToolSkill};
use serde_json::json;

#[cfg(not(test))]
use pyo3::prelude::*;

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

/// Common LangChain tool categories with Argentor wrappers.
///
/// This enum is purely informational -- it maps high-level intent to the
/// LangChain class names and module paths most commonly used for that intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangChainCategory {
    /// Search providers: SerpAPI, Google, Tavily, DuckDuckGo.
    Search,
    /// Database toolkits: SQL, NoSQL.
    Database,
    /// File-system utilities: read, write, list.
    FileSystem,
    /// HTTP / scraping: requests, BeautifulSoup.
    Web,
    /// Code execution: Python REPL, shell.
    Code,
    /// Embeddings providers: OpenAI, HuggingFace.
    Embeddings,
    /// Memory / conversation buffers.
    Memory,
}

impl LangChainCategory {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Database => "database",
            Self::FileSystem => "filesystem",
            Self::Web => "web",
            Self::Code => "code",
            Self::Embeddings => "embeddings",
            Self::Memory => "memory",
        }
    }

    /// Canonical LangChain class names associated with this category.
    pub fn class_names(&self) -> &'static [&'static str] {
        match self {
            Self::Search => &[
                "SerpAPIWrapper",
                "GoogleSearchAPIWrapper",
                "DuckDuckGoSearchRun",
                "TavilySearchResults",
            ],
            Self::Database => &["SQLDatabaseToolkit", "QuerySQLDataBaseTool"],
            Self::FileSystem => &["ReadFileTool", "WriteFileTool", "ListDirectoryTool"],
            Self::Web => &["RequestsGetTool", "RequestsPostTool"],
            Self::Code => &["PythonREPLTool", "ShellTool"],
            Self::Embeddings => &["OpenAIEmbeddings", "HuggingFaceEmbeddings"],
            Self::Memory => &["ConversationBufferMemory", "ConversationSummaryMemory"],
        }
    }

    /// Preferred module path for this category (first in the search order).
    pub fn preferred_module(&self) -> &'static str {
        match self {
            Self::Search | Self::Database | Self::FileSystem | Self::Web | Self::Code => {
                "langchain.tools"
            }
            Self::Embeddings => "langchain.embeddings",
            Self::Memory => "langchain.memory",
        }
    }

    /// All LangChain module paths to try, in order.
    pub fn module_search_path(&self) -> &'static [&'static str] {
        match self {
            Self::Search | Self::Database | Self::FileSystem | Self::Web | Self::Code => &[
                "langchain.tools",
                "langchain_community.tools",
                "langchain_core.tools",
            ],
            Self::Embeddings => &[
                "langchain.embeddings",
                "langchain_community.embeddings",
                "langchain_core.embeddings",
            ],
            Self::Memory => &["langchain.memory", "langchain_community.memory"],
        }
    }

    /// All known categories, for iteration in UIs / tests.
    pub fn all() -> &'static [LangChainCategory] {
        &[
            Self::Search,
            Self::Database,
            Self::FileSystem,
            Self::Web,
            Self::Code,
            Self::Embeddings,
            Self::Memory,
        ]
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Adapter type holding the static surface for loading LangChain tools.
///
/// All methods are associated functions -- no instance state is required.
pub struct LangChainAdapter;

impl LangChainAdapter {
    /// Check if LangChain is installed in the current Python env.
    ///
    /// In test builds this always returns `false` (no Python linked).
    pub fn is_available() -> bool {
        #[cfg(not(test))]
        {
            Python::with_gil(|py| py.import("langchain").is_ok())
        }
        #[cfg(test)]
        {
            false
        }
    }

    /// Get LangChain version (if installed).
    ///
    /// In test builds this always returns `None`.
    pub fn version() -> Option<String> {
        #[cfg(not(test))]
        {
            Python::with_gil(|py| {
                let lc = py.import("langchain").ok()?;
                let v = lc.getattr("__version__").ok()?;
                v.extract::<String>().ok()
            })
        }
        #[cfg(test)]
        {
            None
        }
    }

    /// Load a tool by class name, trying each module in the canonical search
    /// order. Returns an error if the class cannot be found in any module.
    pub fn load_tool(class_name: &str) -> Result<PythonToolSkill, String> {
        Self::load_tool_with_search(class_name, &[
            "langchain.tools",
            "langchain_community.tools",
            "langchain_core.tools",
        ])
    }

    /// Load a tool for a specific [`LangChainCategory`].
    pub fn load_tool_in_category(
        class_name: &str,
        category: LangChainCategory,
    ) -> Result<PythonToolSkill, String> {
        Self::load_tool_with_search(class_name, category.module_search_path())
    }

    /// Internal generic: try each module path until one succeeds.
    fn load_tool_with_search(
        class_name: &str,
        search_path: &[&str],
    ) -> Result<PythonToolSkill, String> {
        if class_name.trim().is_empty() {
            return Err("class_name must not be empty".into());
        }
        if search_path.is_empty() {
            return Err("search_path must not be empty".into());
        }

        #[cfg(not(test))]
        {
            let mut last_err = String::new();
            for module in search_path {
                let cfg = PythonToolConfig {
                    module: (*module).to_string(),
                    callable: class_name.to_string(),
                    name: format!("langchain_{}", crate::dynamic_load::to_snake_case(class_name)),
                    description: format!("LangChain {class_name} ({module})"),
                    parameters_schema: json!({
                        "type": "object",
                        "properties": {
                            "input": { "type": "string", "description": "Tool input" }
                        },
                        "required": ["input"]
                    }),
                };
                let skill = PythonToolSkill::new(cfg);
                match skill.validate() {
                    Ok(()) => return Ok(skill),
                    Err(e) => last_err = e,
                }
            }
            Err(format!(
                "class '{}' not found in any of {:?}: {}",
                class_name, search_path, last_err
            ))
        }

        #[cfg(test)]
        {
            // Test-mode stub: build a config for the first candidate module
            // and return the skill without attempting to import. Callers can
            // still assert the configuration is well-formed.
            let cfg = PythonToolConfig {
                module: search_path[0].to_string(),
                callable: class_name.to_string(),
                name: format!("langchain_{}", crate::dynamic_load::to_snake_case(class_name)),
                description: format!("LangChain {class_name} ({})", search_path[0]),
                parameters_schema: json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string", "description": "Tool input" }
                    },
                    "required": ["input"]
                }),
            };
            Ok(PythonToolSkill::new(cfg))
        }
    }

    /// List all known LangChain tool class names across categories.
    ///
    /// At runtime this could be extended to introspect the installed package,
    /// but the default implementation returns the statically-known canonical
    /// names from [`LangChainCategory::class_names`]. This is intentional:
    /// introspection would require LangChain to be installed, which the tests
    /// do not assume.
    pub fn list_available_tools() -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for cat in LangChainCategory::all() {
            for name in cat.class_names() {
                out.push((*name).to_string());
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// Convert a LangChain tool spec object to an Argentor
    /// [`PythonToolConfig`].
    ///
    /// Expects a Python object that quacks like a LangChain `BaseTool`,
    /// i.e. exposes `.name`, `.description`, and optionally `.args_schema`.
    ///
    /// In test builds this is not exercised -- the function is gated so the
    /// test suite compiles without `&PyAny` in scope.
    #[cfg(not(test))]
    pub fn convert_spec(lc_tool: &Bound<'_, pyo3::PyAny>) -> Result<PythonToolConfig, String> {
        let name: String = lc_tool
            .getattr("name")
            .map_err(|e| format!("missing .name: {e}"))?
            .extract()
            .map_err(|e| format!(".name not a string: {e}"))?;
        let description: String = lc_tool
            .getattr("description")
            .map_err(|e| format!("missing .description: {e}"))?
            .extract()
            .map_err(|e| format!(".description not a string: {e}"))?;
        let class_name: String = lc_tool
            .get_type()
            .getattr("__name__")
            .map_err(|e| format!("missing type name: {e}"))?
            .extract()
            .map_err(|e| format!("type name not a string: {e}"))?;
        let module_name: String = lc_tool
            .get_type()
            .getattr("__module__")
            .map_err(|e| format!("missing module name: {e}"))?
            .extract()
            .map_err(|e| format!("module name not a string: {e}"))?;

        Ok(PythonToolConfig {
            module: module_name,
            callable: class_name,
            name,
            description,
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Tool input" }
                },
                "required": ["input"]
            }),
        })
    }

    /// Test-only stub used to exercise the conversion fallback path without
    /// instantiating a `PyAny`.
    #[cfg(test)]
    pub(crate) fn convert_spec_from_parts(
        module: &str,
        class_name: &str,
        name: &str,
        description: &str,
    ) -> Result<PythonToolConfig, String> {
        if name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        Ok(PythonToolConfig {
            module: module.to_string(),
            callable: class_name.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Tool input" }
                },
                "required": ["input"]
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- LangChainCategory ------------------------------------------------

    #[test]
    fn category_labels_are_stable() {
        assert_eq!(LangChainCategory::Search.label(), "search");
        assert_eq!(LangChainCategory::Database.label(), "database");
        assert_eq!(LangChainCategory::FileSystem.label(), "filesystem");
        assert_eq!(LangChainCategory::Web.label(), "web");
        assert_eq!(LangChainCategory::Code.label(), "code");
        assert_eq!(LangChainCategory::Embeddings.label(), "embeddings");
        assert_eq!(LangChainCategory::Memory.label(), "memory");
    }

    #[test]
    fn category_all_covers_every_variant() {
        // Update this count if new variants are added.
        assert_eq!(LangChainCategory::all().len(), 7);
    }

    #[test]
    fn category_class_names_nonempty() {
        for cat in LangChainCategory::all() {
            assert!(!cat.class_names().is_empty(), "{:?} has no classes", cat);
        }
    }

    #[test]
    fn category_preferred_module_is_in_search_path() {
        for cat in LangChainCategory::all() {
            let pref = cat.preferred_module();
            assert!(
                cat.module_search_path().contains(&pref),
                "{:?} preferred module {} not in search path",
                cat,
                pref
            );
        }
    }

    #[test]
    fn category_hash_eq_copy() {
        use std::collections::HashSet;
        let mut s = HashSet::new();
        s.insert(LangChainCategory::Search);
        s.insert(LangChainCategory::Search);
        assert_eq!(s.len(), 1);
        // Copy trait: value can be used after move.
        let c = LangChainCategory::Search;
        let _d = c;
        let _e = c;
    }

    // --- LangChainAdapter availability ------------------------------------

    #[test]
    fn adapter_reports_unavailable_in_tests() {
        assert!(!LangChainAdapter::is_available());
    }

    #[test]
    fn adapter_version_none_in_tests() {
        assert!(LangChainAdapter::version().is_none());
    }

    // --- LangChainAdapter::load_tool --------------------------------------

    #[test]
    fn load_tool_rejects_empty_class_name() {
        assert!(LangChainAdapter::load_tool("").is_err());
        assert!(LangChainAdapter::load_tool("   ").is_err());
    }

    #[test]
    fn load_tool_returns_stub_skill_in_tests() {
        let skill = LangChainAdapter::load_tool("SerpAPIWrapper").unwrap();
        // Stub uses the first module in the default search path.
        assert_eq!(skill.config().module, "langchain.tools");
        assert_eq!(skill.config().callable, "SerpAPIWrapper");
        assert!(skill.name().starts_with("langchain_"));
    }

    #[test]
    fn load_tool_in_category_uses_category_search_path() {
        let skill = LangChainAdapter::load_tool_in_category(
            "OpenAIEmbeddings",
            LangChainCategory::Embeddings,
        )
        .unwrap();
        assert_eq!(skill.config().module, "langchain.embeddings");
    }

    // --- list_available_tools ---------------------------------------------

    #[test]
    fn list_available_tools_is_sorted_and_unique() {
        let tools = LangChainAdapter::list_available_tools().unwrap();
        assert!(!tools.is_empty());
        let mut sorted = tools.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(tools, sorted);
    }

    #[test]
    fn list_available_tools_contains_serp_api_wrapper() {
        let tools = LangChainAdapter::list_available_tools().unwrap();
        assert!(tools.contains(&"SerpAPIWrapper".to_string()));
    }

    // --- convert_spec_from_parts ------------------------------------------

    #[test]
    fn convert_spec_from_parts_rejects_empty_name() {
        let err = LangChainAdapter::convert_spec_from_parts("m", "C", "", "d").unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn convert_spec_from_parts_builds_valid_config() {
        let c = LangChainAdapter::convert_spec_from_parts(
            "langchain.tools",
            "SerpAPIWrapper",
            "web_search",
            "Search the web",
        )
        .unwrap();
        assert_eq!(c.module, "langchain.tools");
        assert_eq!(c.callable, "SerpAPIWrapper");
        assert_eq!(c.name, "web_search");
        assert!(c.check_fields().is_ok());
    }
}
