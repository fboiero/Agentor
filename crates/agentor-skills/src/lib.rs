//! Skill system with WASM-sandboxed runtime, plugin loading, and registry.
//!
//! This crate defines the skill abstraction used by Agentor agents,
//! including dynamic loading of WASM plugins, a central registry,
//! and markdown-based prompt skills.
//!
//! # Main types
//!
//! - [`Skill`] — Trait that every executable skill implements.
//! - [`SkillDescriptor`] — Metadata describing a skill's name, parameters, and capabilities.
//! - [`SkillRegistry`] — Central registry for discovering and invoking skills.
//! - [`WasmSkillRuntime`] — Wasmtime-based sandbox for running untrusted skill plugins.
//! - [`SkillLoader`] — Loads WASM skill plugins from configuration.
//! - [`MarkdownSkill`] — A skill defined via a Markdown file with YAML frontmatter.

/// WASM skill loader and configuration.
pub mod loader;
/// Markdown-based prompt and callable skills.
pub mod markdown_skill;
/// Plugin system with manifest and event hooks.
pub mod plugin;
/// Central skill registry and tool groups.
pub mod registry;
/// Core skill trait and descriptor.
pub mod skill;
/// Wasmtime-based WASM skill runtime.
pub mod wasm_runtime;

pub use loader::{SkillConfig, SkillLoader};
pub use markdown_skill::{LoadedMarkdownSkills, MarkdownSkill, MarkdownSkillLoader};
pub use plugin::{Plugin, PluginEvent, PluginManifest, PluginRegistry};
pub use registry::{SkillRegistry, ToolGroup};
pub use skill::{Skill, SkillDescriptor};
pub use wasm_runtime::WasmSkillRuntime;
