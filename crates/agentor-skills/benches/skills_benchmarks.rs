use criterion::{black_box, criterion_group, criterion_main, Criterion};

use agentor_core::{AgentorResult, ToolCall, ToolResult};
use agentor_skills::skill::{Skill, SkillDescriptor};
use agentor_skills::vetting::{SkillManifest, SkillVetter};
use agentor_skills::SkillRegistry;
use async_trait::async_trait;
use std::sync::Arc;

struct BenchSkill {
    descriptor: SkillDescriptor,
}

impl BenchSkill {
    fn new(name: &str) -> Self {
        Self {
            descriptor: SkillDescriptor {
                name: name.to_string(),
                description: format!("Bench skill {name}"),
                parameters_schema: serde_json::json!({}),
                required_capabilities: vec![],
            },
        }
    }
}

#[async_trait]
impl Skill for BenchSkill {
    fn descriptor(&self) -> &SkillDescriptor {
        &self.descriptor
    }
    async fn execute(&self, call: ToolCall) -> AgentorResult<ToolResult> {
        Ok(ToolResult::success(&call.id, "ok"))
    }
}

fn bench_registry_lookup(c: &mut Criterion) {
    let mut registry = SkillRegistry::new();
    for i in 0..100 {
        registry.register(Arc::new(BenchSkill::new(&format!("skill_{i}"))));
    }

    c.bench_function("registry lookup (hit)", |b| {
        b.iter(|| registry.get(black_box("skill_50")));
    });

    c.bench_function("registry lookup (miss)", |b| {
        b.iter(|| registry.get(black_box("nonexistent")));
    });

    c.bench_function("list 100 descriptors", |b| {
        b.iter(|| registry.list_descriptors());
    });

    c.bench_function("filter_by_names (10 of 100)", |b| {
        let names: Vec<String> = (0..10).map(|i| format!("skill_{i}")).collect();
        b.iter(|| registry.filter_by_names(black_box(&names)));
    });
}

fn bench_registry_register(c: &mut Criterion) {
    c.bench_function("register 100 skills", |b| {
        b.iter(|| {
            let mut registry = SkillRegistry::new();
            for i in 0..100 {
                registry.register(Arc::new(BenchSkill::new(&format!("skill_{i}"))));
            }
            black_box(registry)
        });
    });
}

fn bench_skill_vetting(c: &mut Criterion) {
    // Minimal WASM module: magic + version
    let wasm_bytes: Vec<u8> = vec![
        0x00, 0x61, 0x73, 0x6D, // magic: \0asm
        0x01, 0x00, 0x00, 0x00, // version: 1
    ];

    c.bench_function("SkillManifest::compute_checksum", |b| {
        b.iter(|| SkillManifest::compute_checksum(black_box(&wasm_bytes)));
    });

    let manifest = SkillManifest {
        name: "bench-skill".into(),
        version: "1.0.0".into(),
        description: "Benchmark skill".into(),
        author: "bench".into(),
        license: None,
        checksum: SkillManifest::compute_checksum(&wasm_bytes),
        capabilities: vec![],
        signature: None,
        signer_key: None,
        min_agentor_version: None,
        tags: vec![],
        repository: None,
    };

    let vetter = SkillVetter::new();
    c.bench_function("SkillVetter::vet (unsigned)", |b| {
        b.iter(|| vetter.vet(black_box(&manifest), black_box(&wasm_bytes)));
    });
}

criterion_group!(
    benches,
    bench_registry_lookup,
    bench_registry_register,
    bench_skill_vetting,
);
criterion_main!(benches);
