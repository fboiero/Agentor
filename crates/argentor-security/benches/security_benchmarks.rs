#![allow(clippy::unwrap_used, clippy::expect_used)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use argentor_security::rbac::{RbacPolicy, Role};
use argentor_security::{is_private_ip, Capability, EncryptedStore, PermissionSet, Sanitizer};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;

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

fn bench_path_canonicalization(c: &mut Criterion) {
    let mut perms = PermissionSet::new();
    perms.grant(Capability::FileRead {
        allowed_paths: vec!["/tmp".into(), "/workspace".into(), "/home/user".into()],
    });
    perms.grant(Capability::FileWrite {
        allowed_paths: vec!["/tmp".into(), "/workspace/output".into()],
    });

    c.bench_function("check_file_read_path (allowed, simple)", |b| {
        b.iter(|| perms.check_file_read_path(black_box(Path::new("/tmp/data.txt"))));
    });

    c.bench_function("check_file_read_path (allowed, nested)", |b| {
        b.iter(|| perms.check_file_read_path(black_box(Path::new("/workspace/src/main.rs"))));
    });

    c.bench_function("check_file_read_path (denied)", |b| {
        b.iter(|| perms.check_file_read_path(black_box(Path::new("/etc/passwd"))));
    });

    c.bench_function("check_file_read_path (traversal attack)", |b| {
        b.iter(|| perms.check_file_read_path(black_box(Path::new("/tmp/../etc/shadow"))));
    });

    c.bench_function("check_file_write_path (allowed)", |b| {
        b.iter(|| perms.check_file_write_path(black_box(Path::new("/tmp/output.txt"))));
    });

    c.bench_function("check_file_write_path (denied)", |b| {
        b.iter(|| perms.check_file_write_path(black_box(Path::new("/etc/shadow"))));
    });

    c.bench_function("check_file_write_path (traversal attack)", |b| {
        b.iter(|| {
            perms.check_file_write_path(black_box(Path::new("/workspace/output/../../etc/shadow")))
        });
    });
}

fn bench_shell_strict(c: &mut Criterion) {
    let mut perms = PermissionSet::new();
    perms.grant(Capability::ShellExec {
        allowed_commands: vec![
            "ls".into(),
            "echo".into(),
            "cat".into(),
            "grep".into(),
            "wc".into(),
        ],
    });

    c.bench_function("check_shell_strict (simple allowed)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("ls -la")));
    });

    c.bench_function("check_shell_strict (pipe, all allowed)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("cat /tmp/file | grep pattern | wc -l")));
    });

    c.bench_function("check_shell_strict (denied command)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("rm -rf /")));
    });

    c.bench_function("check_shell_strict (injection attempt)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("echo hello; rm -rf /")));
    });

    c.bench_function("check_shell_strict (subshell injection)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("echo $(cat /etc/passwd)")));
    });

    c.bench_function("check_shell_strict (chain && allowed)", |b| {
        b.iter(|| perms.check_shell_strict(black_box("echo start && ls -la && echo done")));
    });
}

fn bench_network_ip(c: &mut Criterion) {
    let mut perms = PermissionSet::new();
    perms.grant(Capability::NetworkAccess {
        allowed_hosts: vec!["8.8.8.8".into(), "1.1.1.1".into(), "*".into()],
    });

    c.bench_function("check_network_ip (public IPv4, allowed)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        b.iter(|| perms.check_network_ip(black_box(&ip)));
    });

    c.bench_function("check_network_ip (private IPv4, denied)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        b.iter(|| perms.check_network_ip(black_box(&ip)));
    });

    c.bench_function("check_network_ip (loopback IPv4, denied)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        b.iter(|| perms.check_network_ip(black_box(&ip)));
    });

    c.bench_function("check_network_ip (IPv6 loopback, denied)", |b| {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        b.iter(|| perms.check_network_ip(black_box(&ip)));
    });

    c.bench_function("check_network_ip (public IPv4, wildcard match)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
        b.iter(|| perms.check_network_ip(black_box(&ip)));
    });

    c.bench_function("is_private_ip (public)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4));
        b.iter(|| is_private_ip(black_box(&ip)));
    });

    c.bench_function("is_private_ip (private 10.x)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        b.iter(|| is_private_ip(black_box(&ip)));
    });

    c.bench_function("is_private_ip (private 172.16.x)", |b| {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));
        b.iter(|| is_private_ip(black_box(&ip)));
    });
}

fn bench_has_capability_type(c: &mut Criterion) {
    let mut perms = PermissionSet::new();
    perms.grant(Capability::FileRead {
        allowed_paths: vec!["/tmp".into()],
    });
    perms.grant(Capability::FileWrite {
        allowed_paths: vec!["/tmp".into()],
    });
    perms.grant(Capability::NetworkAccess {
        allowed_hosts: vec!["example.com".into()],
    });
    perms.grant(Capability::ShellExec {
        allowed_commands: vec!["ls".into(), "echo".into()],
    });
    perms.grant(Capability::EnvRead {
        allowed_vars: vec!["HOME".into(), "PATH".into()],
    });
    perms.grant(Capability::DatabaseQuery);
    perms.grant(Capability::BrowserAccess {
        allowed_domains: vec!["example.com".into()],
    });

    c.bench_function("has_capability_type (hit, first)", |b| {
        b.iter(|| perms.has_capability_type(black_box("file_read")));
    });

    c.bench_function("has_capability_type (hit, last)", |b| {
        b.iter(|| perms.has_capability_type(black_box("browser_access")));
    });

    c.bench_function("has_capability_type (miss)", |b| {
        b.iter(|| perms.has_capability_type(black_box("nonexistent_type")));
    });

    c.bench_function("has_capability_type (database_query)", |b| {
        b.iter(|| perms.has_capability_type(black_box("database_query")));
    });
}

criterion_group!(
    benches,
    bench_rbac_evaluation,
    bench_permission_check,
    bench_encrypted_store,
    bench_sanitizer,
    bench_path_canonicalization,
    bench_shell_strict,
    bench_network_ip,
    bench_has_capability_type,
);
criterion_main!(benches);
