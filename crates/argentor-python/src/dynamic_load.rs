//! Dynamic loading of Python callables as Argentor skills.
//!
//! Allows wrapping any Python function (including LangChain tools)
//! as an Argentor skill that runs through the Python interpreter.
//!
//! # Overview
//!
//! The [`PythonToolSkill`] type wraps a Python callable referenced by `(module,
//! callable)` pair. At call time, the skill:
//!
//! 1. Acquires the GIL through [`Python::with_gil`].
//! 2. Imports `module` via `py.import(...)`.
//! 3. Retrieves `callable` from the module namespace.
//! 4. Converts the JSON `args` into a Python dict (or positional args when the
//!    JSON is an array).
//! 5. Invokes the callable and coerces the return value back to a string.
//!
//! The actual PyO3 calls are gated behind `#[cfg(not(test))]` so the test
//! suite can verify the configuration/error-handling surface without requiring
//! a live Python interpreter. In production builds (`cargo check`,
//! `maturin develop`, `maturin build`) the real interpreter is used.
//!
//! # Example (runtime only -- not exercised in unit tests)
//!
//! ```ignore
//! use argentor_python::dynamic_load::{PythonToolConfig, PythonToolSkill};
//! use serde_json::json;
//!
//! let cfg = PythonToolConfig {
//!     module: "math".into(),
//!     callable: "sqrt".into(),
//!     name: "sqrt".into(),
//!     description: "Square root".into(),
//!     parameters_schema: json!({
//!         "type": "object",
//!         "properties": {"x": {"type": "number"}},
//!         "required": ["x"],
//!     }),
//! };
//! let skill = PythonToolSkill::new(cfg);
//! skill.validate().unwrap();
//! let out = skill.call(&json!([16])).unwrap();
//! assert_eq!(out, "4.0");
//! ```

#![allow(dead_code)]

use serde_json::{json, Value};

#[cfg(not(test))]
use pyo3::prelude::*;
#[cfg(not(test))]
use pyo3::types::{PyDict, PyList};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a dynamically loaded Python tool.
///
/// A config describes *how* to import and invoke a Python callable so that it
/// can be exposed as an Argentor skill. It never contains the callable itself
/// -- resolution happens on each `call()` so that the config remains `Send +
/// Sync` without wrapping [`PyObject`] references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonToolConfig {
    /// Python module to import (e.g., `"langchain.tools"` or a custom module).
    pub module: String,
    /// Function/class name within the module.
    pub callable: String,
    /// Tool name as exposed to the agent.
    pub name: String,
    /// Tool description for the LLM.
    pub description: String,
    /// JSON Schema for tool parameters.
    pub parameters_schema: Value,
}

impl PythonToolConfig {
    /// Build a `PythonToolConfig` from minimal fields. Useful in tests and
    /// simple programmatic registration where a full schema is not required.
    pub fn minimal(module: &str, callable: &str, name: &str, description: &str) -> Self {
        Self {
            module: module.to_string(),
            callable: callable.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters_schema: json!({ "type": "object", "properties": {} }),
        }
    }

    /// Fully-qualified dotted path of the callable (`module.callable`).
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.module, self.callable)
    }

    /// Quick structural sanity check -- returns an error when required fields
    /// are empty or clearly malformed. This is a lightweight, Python-free
    /// check; the full import is performed in [`PythonToolSkill::validate`].
    pub fn check_fields(&self) -> Result<(), String> {
        if self.module.trim().is_empty() {
            return Err("module must not be empty".into());
        }
        if self.callable.trim().is_empty() {
            return Err("callable must not be empty".into());
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        if !self.parameters_schema.is_object() {
            return Err("parameters_schema must be a JSON object".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Skill wrapper
// ---------------------------------------------------------------------------

/// A skill that wraps a Python callable.
///
/// The underlying Python function is resolved lazily on every `call()` -- this
/// keeps the wrapper `Send + Sync` (no retained `PyObject`) and allows the
/// interpreter to be restarted between invocations if necessary.
#[derive(Debug, Clone)]
pub struct PythonToolSkill {
    config: PythonToolConfig,
}

impl PythonToolSkill {
    /// Wrap the given config as a skill. Does not touch the Python
    /// interpreter. Call [`validate`] for import-time checks.
    ///
    /// [`validate`]: Self::validate
    pub fn new(config: PythonToolConfig) -> Self {
        Self { config }
    }

    /// Access the underlying configuration.
    pub fn config(&self) -> &PythonToolConfig {
        &self.config
    }

    /// Tool name as exposed to the agent.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Tool description for the LLM.
    pub fn description(&self) -> &str {
        &self.config.description
    }

    /// Verify the Python callable is importable and callable.
    ///
    /// In test builds this only performs a structural field check -- real
    /// import happens at runtime when linked against the Python interpreter.
    pub fn validate(&self) -> Result<(), String> {
        self.config.check_fields()?;
        #[cfg(not(test))]
        {
            validate_with_py(&self.config)?;
        }
        Ok(())
    }

    /// Execute the Python callable with the given JSON arguments.
    ///
    /// JSON conversion rules:
    ///
    /// * JSON object -> Python keyword arguments (`**kwargs`)
    /// * JSON array  -> Python positional arguments (`*args`)
    /// * anything else -> single positional argument
    ///
    /// The return value is coerced via `str(result)` for a uniform string
    /// interface.
    pub fn call(&self, args: &Value) -> Result<String, String> {
        self.config.check_fields()?;
        #[cfg(not(test))]
        {
            call_with_py(&self.config, args)
        }
        #[cfg(test)]
        {
            // In test builds there is no interpreter. Surface a deterministic
            // error so callers can still exercise the wrapper plumbing.
            Err(format!(
                "python runtime unavailable in test build -- would have called {} with args {}",
                self.config.qualified_name(),
                args
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers (production only)
// ---------------------------------------------------------------------------

#[cfg(not(test))]
fn validate_with_py(cfg: &PythonToolConfig) -> Result<(), String> {
    Python::with_gil(|py| {
        let module = py
            .import(cfg.module.as_str())
            .map_err(|e| format!("failed to import '{}': {}", cfg.module, e))?;
        let attr = module
            .getattr(cfg.callable.as_str())
            .map_err(|e| format!("module '{}' has no attribute '{}': {}", cfg.module, cfg.callable, e))?;
        if !attr.is_callable() {
            return Err(format!("{}.{} is not callable", cfg.module, cfg.callable));
        }
        Ok(())
    })
}

#[cfg(not(test))]
fn call_with_py(cfg: &PythonToolConfig, args: &Value) -> Result<String, String> {
    Python::with_gil(|py| {
        let module = py
            .import(cfg.module.as_str())
            .map_err(|e| format!("failed to import '{}': {}", cfg.module, e))?;
        let callable = module
            .getattr(cfg.callable.as_str())
            .map_err(|e| format!("missing attribute '{}': {}", cfg.callable, e))?;

        let result = match args {
            Value::Object(map) => {
                let kwargs = PyDict::new(py);
                for (k, v) in map {
                    let py_val = json_to_py(py, v)?;
                    kwargs
                        .set_item(k, py_val)
                        .map_err(|e| format!("kwarg set failed: {e}"))?;
                }
                callable
                    .call((), Some(&kwargs))
                    .map_err(|e| format!("python call failed: {e}"))?
            }
            Value::Array(arr) => {
                let list = PyList::empty(py);
                for v in arr {
                    let py_val = json_to_py(py, v)?;
                    list.append(py_val).map_err(|e| format!("arg append failed: {e}"))?;
                }
                let tup = list.to_tuple();
                callable
                    .call(tup, None)
                    .map_err(|e| format!("python call failed: {e}"))?
            }
            other => {
                let py_val = json_to_py(py, other)?;
                callable
                    .call1((py_val,))
                    .map_err(|e| format!("python call failed: {e}"))?
            }
        };

        let as_str = result
            .str()
            .map_err(|e| format!("str() failed on result: {e}"))?;
        as_str
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| format!("utf-8 decode failed: {e}"))
    })
}

#[cfg(not(test))]
fn json_to_py<'py>(py: Python<'py>, value: &Value) -> Result<Bound<'py, pyo3::PyAny>, String> {
    use pyo3::IntoPyObject;
    match value {
        Value::Null => Ok(py.None().into_bound(py)),
        Value::Bool(b) => b
            .into_pyobject(py)
            .map(|b| b.to_owned().into_any())
            .map_err(|e| format!("bool conv: {e}")),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)
                    .map(|o| o.into_any())
                    .map_err(|e| format!("int conv: {e}"))
            } else if let Some(f) = n.as_f64() {
                f.into_pyobject(py)
                    .map(|o| o.into_any())
                    .map_err(|e| format!("float conv: {e}"))
            } else {
                Err(format!("unsupported numeric value: {n}"))
            }
        }
        Value::String(s) => s
            .into_pyobject(py)
            .map(|o| o.into_any())
            .map_err(|e| format!("str conv: {e}")),
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                let v = json_to_py(py, item)?;
                list.append(v).map_err(|e| format!("list append: {e}"))?;
            }
            Ok(list.into_any())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_v = json_to_py(py, v)?;
                dict.set_item(k, py_v)
                    .map_err(|e| format!("dict set: {e}"))?;
            }
            Ok(dict.into_any())
        }
    }
}

// ---------------------------------------------------------------------------
// Public free functions
// ---------------------------------------------------------------------------

/// Helper to load a LangChain tool by class name.
///
/// Assumes `langchain` is installed in the active Python environment and that
/// the class lives under `langchain.tools.<tool_name>`.
pub fn load_langchain_tool(tool_name: &str) -> Result<PythonToolSkill, String> {
    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".into());
    }
    let cfg = PythonToolConfig {
        module: "langchain.tools".into(),
        callable: tool_name.to_string(),
        name: format!("langchain_{}", to_snake_case(tool_name)),
        description: format!("LangChain tool wrapper for {tool_name}"),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Input string for the tool" }
            },
            "required": ["input"]
        }),
    };
    let skill = PythonToolSkill::new(cfg);
    skill.validate()?;
    Ok(skill)
}

/// Helper to scan a Python module for all callables whose names match an
/// optional prefix.
///
/// In test builds this only performs the structural prefix check against a
/// stub list. At runtime it imports the module and uses `dir()` + `callable()`.
pub fn discover_python_tools(
    module_name: &str,
    prefix: Option<&str>,
) -> Result<Vec<PythonToolConfig>, String> {
    if module_name.trim().is_empty() {
        return Err("module_name must not be empty".into());
    }

    #[cfg(not(test))]
    {
        Python::with_gil(|py| {
            let module = py
                .import(module_name)
                .map_err(|e| format!("failed to import '{}': {}", module_name, e))?;
            let names: Vec<String> = module
                .dir()
                .map_err(|e| format!("dir() failed: {e}"))?
                .iter()
                .filter_map(|n| n.extract::<String>().ok())
                .collect();

            let mut out = Vec::new();
            for n in names {
                if n.starts_with('_') {
                    continue;
                }
                if let Some(pfx) = prefix {
                    if !n.starts_with(pfx) {
                        continue;
                    }
                }
                if let Ok(attr) = module.getattr(n.as_str()) {
                    if attr.is_callable() {
                        out.push(PythonToolConfig::minimal(
                            module_name,
                            &n,
                            &format!("{module_name}_{n}"),
                            &format!("Discovered callable {module_name}.{n}"),
                        ));
                    }
                }
            }
            Ok(out)
        })
    }

    #[cfg(test)]
    {
        // Deterministic stub: return the prefix filter as a zero-length vec
        // when no prefix is provided, and a single dummy entry when it is.
        let _ = prefix; // suppress warning
        Ok(Vec::new())
    }
}

/// Convert CamelCase / PascalCase to snake_case. Pure function, fully
/// test-covered.
pub(crate) fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- PythonToolConfig -------------------------------------------------

    #[test]
    fn config_minimal_sets_defaults() {
        let c = PythonToolConfig::minimal("m", "f", "n", "d");
        assert_eq!(c.module, "m");
        assert_eq!(c.callable, "f");
        assert_eq!(c.name, "n");
        assert_eq!(c.description, "d");
        assert!(c.parameters_schema.is_object());
    }

    #[test]
    fn config_qualified_name() {
        let c = PythonToolConfig::minimal("langchain.tools", "SerpAPIWrapper", "x", "");
        assert_eq!(c.qualified_name(), "langchain.tools.SerpAPIWrapper");
    }

    #[test]
    fn config_check_fields_accepts_valid() {
        let c = PythonToolConfig::minimal("m", "f", "n", "d");
        assert!(c.check_fields().is_ok());
    }

    #[test]
    fn config_check_fields_rejects_empty_module() {
        let mut c = PythonToolConfig::minimal("m", "f", "n", "d");
        c.module = "".into();
        assert!(c.check_fields().is_err());
    }

    #[test]
    fn config_check_fields_rejects_empty_callable() {
        let mut c = PythonToolConfig::minimal("m", "f", "n", "d");
        c.callable = "   ".into();
        let err = c.check_fields().unwrap_err();
        assert!(err.contains("callable"));
    }

    #[test]
    fn config_check_fields_rejects_empty_name() {
        let mut c = PythonToolConfig::minimal("m", "f", "n", "d");
        c.name = "".into();
        assert!(c.check_fields().is_err());
    }

    #[test]
    fn config_check_fields_rejects_non_object_schema() {
        let mut c = PythonToolConfig::minimal("m", "f", "n", "d");
        c.parameters_schema = json!([1, 2, 3]);
        let err = c.check_fields().unwrap_err();
        assert!(err.contains("parameters_schema"));
    }

    #[test]
    fn config_clone_eq() {
        let c = PythonToolConfig::minimal("m", "f", "n", "d");
        let c2 = c.clone();
        assert_eq!(c, c2);
    }

    #[test]
    fn config_is_serializable_via_json() {
        let c = PythonToolConfig {
            module: "math".into(),
            callable: "sqrt".into(),
            name: "sqrt".into(),
            description: "square root".into(),
            parameters_schema: json!({"type":"object","properties":{"x":{"type":"number"}}}),
        };
        // Round-trip the schema alone (config itself is not Serialize; the
        // schema field is, which is what matters for LLM tool spec emission).
        let s = serde_json::to_string(&c.parameters_schema).unwrap();
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v, c.parameters_schema);
    }

    // --- PythonToolSkill --------------------------------------------------

    #[test]
    fn skill_new_stores_config() {
        let c = PythonToolConfig::minimal("math", "sqrt", "sqrt", "square root");
        let s = PythonToolSkill::new(c.clone());
        assert_eq!(s.config(), &c);
        assert_eq!(s.name(), "sqrt");
        assert_eq!(s.description(), "square root");
    }

    #[test]
    fn skill_validate_passes_structural_check_in_tests() {
        let s = PythonToolSkill::new(PythonToolConfig::minimal("m", "f", "n", "d"));
        assert!(s.validate().is_ok());
    }

    #[test]
    fn skill_validate_rejects_bad_config() {
        let mut c = PythonToolConfig::minimal("m", "f", "n", "d");
        c.module = "".into();
        let s = PythonToolSkill::new(c);
        assert!(s.validate().is_err());
    }

    #[test]
    fn skill_call_surfaces_runtime_unavailable_in_tests() {
        let s = PythonToolSkill::new(PythonToolConfig::minimal("math", "sqrt", "sqrt", "d"));
        let err = s.call(&json!({"x": 16})).unwrap_err();
        assert!(err.contains("python runtime unavailable"));
        assert!(err.contains("math.sqrt"));
    }

    #[test]
    fn skill_call_still_validates_before_dispatch() {
        let mut c = PythonToolConfig::minimal("math", "sqrt", "sqrt", "d");
        c.callable = "".into();
        let s = PythonToolSkill::new(c);
        let err = s.call(&json!({})).unwrap_err();
        assert!(err.contains("callable"));
    }

    #[test]
    fn skill_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PythonToolSkill>();
    }

    // --- load_langchain_tool ----------------------------------------------

    #[test]
    fn load_langchain_tool_rejects_empty_name() {
        assert!(load_langchain_tool("").is_err());
        assert!(load_langchain_tool("   ").is_err());
    }

    // --- discover_python_tools --------------------------------------------

    #[test]
    fn discover_rejects_empty_module_name() {
        assert!(discover_python_tools("", None).is_err());
    }

    #[test]
    fn discover_returns_empty_in_tests() {
        let out = discover_python_tools("os", Some("path")).unwrap();
        assert!(out.is_empty());
    }

    // --- to_snake_case ----------------------------------------------------

    #[test]
    fn snake_case_basic() {
        assert_eq!(to_snake_case("CamelCase"), "camel_case");
    }

    #[test]
    fn snake_case_already_lower() {
        assert_eq!(to_snake_case("already_lower"), "already_lower");
    }

    #[test]
    fn snake_case_pascal_acronym() {
        // Naive conversion: each uppercase becomes its own word.
        assert_eq!(to_snake_case("HTTPServer"), "h_t_t_p_server");
    }

    #[test]
    fn snake_case_empty() {
        assert_eq!(to_snake_case(""), "");
    }
}
