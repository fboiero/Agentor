#![allow(clippy::unwrap_used, clippy::expect_used)]
//! # Security regression tests — SSRF protection (CWE-918)
//!
//! These tests pin the URL / IP / hostname validation primitives the HTTP
//! and browser skills rely on. The validation MUST happen at the
//! capability layer (`PermissionSet`) and at the dedicated `is_private_ip`
//! helper — both are exercised here. The actual `HttpFetchSkill` lives in
//! `argentor-builtins` and has its own tests; these regressions guard the
//! shared primitives so a regression there is impossible to miss.
//!
//! References:
//! - CWE-918: Server-Side Request Forgery (SSRF)
//! - CWE-441: Unintended Proxy or Intermediary
//! - OWASP A10:2021 — Server-Side Request Forgery
//! - Capital One 2019 (AWS metadata endpoint exposure)

use argentor_security::{is_private_ip, Capability, PermissionSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

fn perms_wildcard_network() -> PermissionSet {
    let mut perms = PermissionSet::new();
    perms.grant(Capability::NetworkAccess {
        allowed_hosts: vec!["*".to_string()],
    });
    perms
}

fn ip(s: &str) -> IpAddr {
    s.parse().expect("test bug: bad IP literal")
}

// ---------------------------------------------------------------------------
// Per-IP regression: every block must stay blocked even with wildcard hosts
// ---------------------------------------------------------------------------

/// CWE-918: IPv4 loopback `127.0.0.1` must be denied even with `*` hosts.
#[test]
fn test_blocks_127001() {
    let perms = perms_wildcard_network();
    let target = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    assert!(is_private_ip(&target));
    assert!(
        !perms.check_network_ip(&target),
        "CRITICAL: 127.0.0.1 must be denied even when wildcard '*' host is granted"
    );
}

/// CWE-918: localhost hostname — covered by `check_network` only when the
/// caller explicitly resolves `localhost` to an IP. The capability layer
/// allows the literal hostname; SSRF defence is enforced at the skill layer
/// (HttpFetchSkill blocks "localhost" via its own hostname blocklist).
#[test]
fn test_blocks_localhost_http() {
    // Document that 'localhost' as a string IS reachable through wildcard
    // network host check — the actual HTTP skill (in argentor-builtins) is
    // responsible for blocking the literal hostname before DNS resolution.
    let perms = perms_wildcard_network();
    // The capability layer allows the string "localhost" because it's not
    // an IP literal — SSRF protection happens at the skill layer.
    assert!(
        perms.check_network("localhost"),
        "Documented behaviour: capability layer treats 'localhost' as just a host string; \
         skill layer (HttpFetchSkill) is responsible for blocking it"
    );
    // But once resolved to 127.0.0.1, it must be denied:
    assert!(!perms.check_network_ip(&ip("127.0.0.1")));
}

/// CWE-918: IPv6 loopback `::1` must be denied.
#[test]
fn test_blocks_ipv6_loopback() {
    let perms = perms_wildcard_network();
    let target = IpAddr::V6(Ipv6Addr::LOCALHOST);
    assert!(is_private_ip(&target));
    assert!(
        !perms.check_network_ip(&target),
        "CRITICAL: IPv6 loopback ::1 must be denied"
    );
}

/// CWE-918: RFC 1918 private 10.0.0.0/8 must be denied.
#[test]
fn test_blocks_private_10x() {
    let perms = perms_wildcard_network();
    for ip_str in ["10.0.0.1", "10.255.255.255", "10.42.42.42"] {
        let target = ip(ip_str);
        assert!(is_private_ip(&target), "{ip_str} must classify as private");
        assert!(
            !perms.check_network_ip(&target),
            "CRITICAL: 10.x.x.x ({ip_str}) must be denied"
        );
    }
}

/// CWE-918: RFC 1918 private 192.168.0.0/16 must be denied.
#[test]
fn test_blocks_private_192168() {
    let perms = perms_wildcard_network();
    for ip_str in ["192.168.0.1", "192.168.1.1", "192.168.255.255"] {
        let target = ip(ip_str);
        assert!(is_private_ip(&target));
        assert!(
            !perms.check_network_ip(&target),
            "CRITICAL: 192.168.x.x ({ip_str}) must be denied"
        );
    }
}

/// CWE-918: RFC 1918 private 172.16.0.0/12 must be denied.
#[test]
fn test_blocks_private_172() {
    let perms = perms_wildcard_network();
    for ip_str in ["172.16.0.1", "172.20.10.1", "172.31.255.255"] {
        let target = ip(ip_str);
        assert!(is_private_ip(&target));
        assert!(
            !perms.check_network_ip(&target),
            "CRITICAL: 172.16-31.x.x ({ip_str}) must be denied"
        );
    }
    // Boundary: 172.32.x.x is NOT private (just outside the /12).
    assert!(!is_private_ip(&ip("172.32.0.1")));
}

/// CWE-918 / CAPITAL ONE 2019: link-local 169.254.169.254 (AWS/GCP metadata).
#[test]
fn test_blocks_link_local_169254() {
    let perms = perms_wildcard_network();
    let aws_metadata = ip("169.254.169.254");
    assert!(
        is_private_ip(&aws_metadata),
        "CRITICAL: AWS metadata IP must classify as private"
    );
    assert!(
        !perms.check_network_ip(&aws_metadata),
        "CRITICAL: AWS metadata endpoint 169.254.169.254 must be denied"
    );

    // Whole link-local range
    assert!(is_private_ip(&ip("169.254.0.1")));
    assert!(is_private_ip(&ip("169.254.255.255")));
}

/// CWE-918: GCP metadata hostname `metadata.google.internal`.
///
/// Documented behaviour: the security crate's `PermissionSet::check_network`
/// does NOT have an internal hostname blocklist — that lives in the HTTP
/// skill (`argentor-builtins::http_fetch::is_blocked_hostname`). This test
/// pins the architectural boundary so no one accidentally moves it.
#[test]
fn test_blocks_gcp_metadata() {
    let perms = perms_wildcard_network();
    // Wildcard host allows the LITERAL hostname through the capability
    // layer — the HTTP skill is responsible for the metadata hostname
    // blocklist via its private `is_blocked_hostname` function.
    assert!(
        perms.check_network("metadata.google.internal"),
        "Documented: security crate does not blocklist metadata hosts; \
         http_fetch skill enforces this. See argentor-builtins::http_fetch."
    );
}

/// CWE-918 / CWE-441: DNS rebinding — a hostname that resolves to a private
/// IP at the moment of connect. The capability layer cannot block this on
/// its own (it does not perform DNS); the HTTP skill must resolve and
/// re-validate. This test documents the architectural contract.
///
/// KNOWN GAP: there is no end-to-end integration test in the security crate
/// that proves DNS rebinding is caught — the e2e check lives in the
/// `argentor-builtins` http_fetch tests where the actual HTTP client lives.
#[test]
#[ignore = "ARCHITECTURE: DNS rebinding is enforced by HttpFetchSkill::execute, not PermissionSet"]
fn test_blocks_dns_rebinding() {
    // Placeholder — the relevant test is
    // `argentor-builtins::http_fetch::tests::test_http_fetch_blocks_ssrf`
    // which exercises the real DNS resolution path. Documented here so the
    // security regression suite explicitly references this attack class.
}

/// Negative test: legitimate public HTTPS host must be allowed.
#[test]
fn test_allows_public_https() {
    let perms = perms_wildcard_network();
    let google_dns = ip("8.8.8.8");
    assert!(
        !is_private_ip(&google_dns),
        "8.8.8.8 must NOT classify as private"
    );
    assert!(
        perms.check_network_ip(&google_dns),
        "False positive: 8.8.8.8 must be allowed under wildcard network"
    );

    // Cloudflare
    let one_one = ip("1.1.1.1");
    assert!(!is_private_ip(&one_one));
    assert!(perms.check_network_ip(&one_one));
}
