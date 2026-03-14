#![allow(clippy::unwrap_used, clippy::expect_used)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use argentor_security::rbac::{RbacPolicy, Role};
use argentor_security::{EncryptedStore, PermissionSet, Sanitizer};

fn bench_rbac_evaluation(c: &mut Criterion) {
    let policy = RbacPolicy::with_defaults();

    c.bench_function("RBAC evaluate admin", |b| {
        b.iter(|| policy.evaluate(black_box(&Role::Admin), black_box("shell_exec")));
    });

    c.bench_function("RBAC evaluate operator (denied)", |b| {
        b.iter(|| policy.evaluate(black_box(&Role::Operator), black_box("shell_exec")));
    });

    c.bench_function("RBAC evaluate viewer (allowed)", |b| {
        b.iter(|| policy.evaluate(black_box(&Role::Viewer), black_box("help")));
    });
}

fn bench_permission_check(c: &mut Criterion) {
    let mut perms = PermissionSet::new();
    perms.grant(argentor_security::Capability::FileRead {
        allowed_paths: vec![
            "/tmp".into(),
            "/workspace".into(),
            "/home/user".into(),
            "/var/data".into(),
        ],
    });
    perms.grant(argentor_security::Capability::NetworkAccess {
        allowed_hosts: vec![
            "api.anthropic.com".into(),
            "api.openai.com".into(),
            "*.example.com".into(),
        ],
    });

    c.bench_function("check_file_read (match)", |b| {
        b.iter(|| perms.check_file_read(black_box("/workspace/src/main.rs")));
    });

    c.bench_function("check_file_read (no match)", |b| {
        b.iter(|| perms.check_file_read(black_box("/etc/passwd")));
    });

    c.bench_function("check_network (match)", |b| {
        b.iter(|| perms.check_network(black_box("api.anthropic.com")));
    });
}

fn bench_encrypted_store(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = EncryptedStore::new(dir.path(), "benchmark-passphrase").expect("store");

    // Small value
    let small = b"small secret value";
    c.bench_function("encrypt+store 18 bytes", |b| {
        b.iter(|| store.put(black_box("small_key"), black_box(small)));
    });

    store.put("read_key", small).expect("put");
    c.bench_function("read+decrypt 18 bytes", |b| {
        b.iter(|| store.get(black_box("read_key")));
    });

    // Larger value (4KB)
    let large = vec![0xABu8; 4096];
    c.bench_function("encrypt+store 4KB", |b| {
        b.iter(|| store.put(black_box("large_key"), black_box(&large)));
    });

    store.put("read_large", &large).expect("put");
    c.bench_function("read+decrypt 4KB", |b| {
        b.iter(|| store.get(black_box("read_large")));
    });
}

fn bench_sanitizer(c: &mut Criterion) {
    let sanitizer = Sanitizer::new(10_000);
    let clean_input = "Hello, this is a clean user input without any issues.";
    let dirty_input = "Hello\x00\x01\x02\x1b[31mred\x1b[0m world";

    c.bench_function("sanitize clean input", |b| {
        b.iter(|| sanitizer.sanitize(black_box(clean_input)));
    });

    c.bench_function("sanitize dirty input", |b| {
        b.iter(|| sanitizer.sanitize(black_box(dirty_input)));
    });
}

criterion_group!(
    benches,
    bench_rbac_evaluation,
    bench_permission_check,
    bench_encrypted_store,
    bench_sanitizer,
);
criterion_main!(benches);
