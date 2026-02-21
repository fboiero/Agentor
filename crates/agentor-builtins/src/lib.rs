pub mod file_read;
pub mod file_write;
pub mod http_fetch;
pub mod shell;

pub use file_read::FileReadSkill;
pub use file_write::FileWriteSkill;
pub use http_fetch::HttpFetchSkill;
pub use shell::ShellSkill;

use agentor_skills::SkillRegistry;
use std::sync::Arc;

/// Register all built-in skills into the given registry.
pub fn register_builtins(registry: &mut SkillRegistry) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
}
