pub mod browser;
pub mod file_read;
pub mod file_write;
pub mod http_fetch;
pub mod memory;
pub mod shell;

pub use browser::BrowserSkill;
pub use file_read::FileReadSkill;
pub use file_write::FileWriteSkill;
pub use http_fetch::HttpFetchSkill;
pub use memory::{MemorySearchSkill, MemoryStoreSkill};
pub use shell::ShellSkill;

use agentor_memory::{EmbeddingProvider, VectorStore};
use agentor_skills::SkillRegistry;
use std::sync::Arc;

/// Register all built-in skills into the given registry.
/// Uses the provided vector store and embedding provider for memory skills.
pub fn register_builtins_with_memory(
    registry: &mut SkillRegistry,
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn EmbeddingProvider>,
) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(MemoryStoreSkill::new(
        store.clone(),
        embedder.clone(),
    )));
    registry.register(Arc::new(MemorySearchSkill::new(store, embedder)));
}

/// Register built-in skills without memory (backwards compatible).
pub fn register_builtins(registry: &mut SkillRegistry) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
}
