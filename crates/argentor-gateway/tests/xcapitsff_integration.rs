#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use argentor_gateway::xcapitsff::{xcapitsff_router, XcapitConfig, XcapitState};
use argentor_security::{AuditLog, PermissionSet};
use argentor_skills::SkillRegistry;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Helper: build a test XcapitSFF server on a random port, returning the base URL.
async fn start_test_server() -> (String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(AuditLog::new(tmp.path().join("audit")));
    let skills = Arc::new(SkillRegistry::new());
    let permissions = PermissionSet::new();
    let config = XcapitConfig::default();

    let state = Arc::new(XcapitState::new(skills, permissions, audit, config));
    let app = xcapitsff_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Small yield to let the server task start accepting connections.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, tmp)
}

// ---------------------------------------------------------------------------
// 1. GET /api/v1/health returns 200 with xcapitsff status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base_url}/api/v1/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    // Extended health response must have status, version, checks, and xcapitsff fields.
    assert!(body["status"].is_string(), "Expected status field");
    assert!(body["version"].is_string(), "Expected version field");
    assert!(body["checks"].is_object(), "Expected checks object");
    assert!(
        body["xcapitsff"].is_object(),
        "Expected xcapitsff health object"
    );
    // The xcapitsff status starts as "unknown" since no health loop ran.
    assert_eq!(body["xcapitsff"]["status"], "unknown");
}

// ---------------------------------------------------------------------------
// 2. GET /api/v1/agent/profiles returns list of 4 profiles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_profiles() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base_url}/api/v1/agent/profiles"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["total"], 4);

    let profiles = body["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 4);

    // Verify the expected roles exist.
    let roles: Vec<&str> = profiles
        .iter()
        .map(|p| p["role"].as_str().unwrap())
        .collect();
    assert!(
        roles.contains(&"sales_qualifier"),
        "Missing sales_qualifier"
    );
    assert!(
        roles.contains(&"outreach_composer"),
        "Missing outreach_composer"
    );
    assert!(
        roles.contains(&"support_responder"),
        "Missing support_responder"
    );
    assert!(roles.contains(&"ticket_router"), "Missing ticket_router");

    // Each profile should have key fields.
    for profile in profiles {
        assert!(profile["model"].is_string());
        assert!(profile["temperature"].is_number());
        assert!(profile["max_tokens"].is_number());
        assert!(profile["system_prompt_preview"].is_string());
    }
}

// ---------------------------------------------------------------------------
// 3. POST /api/v1/agent/run-task with unknown role returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_task_unknown_role_returns_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base_url}/api/v1/agent/run-task"))
        .json(&serde_json::json!({
            "agent_role": "nonexistent_role",
            "context": "Some context"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.unwrap();
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("Unknown agent_role"),
        "Error message should mention unknown role, got: {error}"
    );
    assert!(
        error.contains("nonexistent_role"),
        "Error message should include the requested role name"
    );
}

// ---------------------------------------------------------------------------
// 4. POST /api/v1/agent/run-task with empty context — guardrail passes short text
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_task_empty_context() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    // Empty context should pass guardrails (no PII, no injection, no toxicity)
    // but the agent will fail because there is no real LLM backend.
    let resp = client
        .post(format!("{base_url}/api/v1/agent/run-task"))
        .json(&serde_json::json!({
            "agent_role": "sales_qualifier",
            "context": ""
        }))
        .send()
        .await
        .unwrap();

    // The status should NOT be 400 (guardrail block). It will be 500 because
    // there is no real LLM to call, which proves the request passed validation.
    let status = resp.status().as_u16();
    assert_ne!(
        status, 400,
        "Empty context should not be blocked by guardrails"
    );
    // Either 200 (demo) or 500 (LLM failure) are acceptable.
    assert!(
        status == 200 || status == 500,
        "Expected 200 or 500, got {status}"
    );
}

// ---------------------------------------------------------------------------
// 5. POST /api/v1/agent/batch with empty tasks returns 200 with 0 results
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_batch_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base_url}/api/v1/agent/batch"))
        .json(&serde_json::json!({
            "tasks": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["total"], 0);
    assert_eq!(body["succeeded"], 0);
    assert_eq!(body["failed"], 0);
    assert_eq!(body["total_tokens"], 0);

    let results = body["results"].as_array().unwrap();
    assert!(results.is_empty(), "Empty batch should produce no results");
}

// ---------------------------------------------------------------------------
// 6. POST /api/v1/agent/evaluate returns quality scores
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_evaluate_endpoint() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base_url}/api/v1/agent/evaluate"))
        .json(&serde_json::json!({
            "context": "What is the capital of Argentina?",
            "response": "The capital of Argentina is Buenos Aires. It is the largest city in the country and serves as the political, economic, and cultural center.",
            "criteria": ["relevance", "helpfulness", "tone"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    // Must have overall_score between 0 and 1.
    let overall = body["overall_score"].as_f64().unwrap();
    assert!(
        (0.0..=1.0).contains(&overall),
        "overall_score should be between 0.0 and 1.0, got {overall}"
    );

    // Must have by_criteria with the requested criteria.
    let by_criteria = body["by_criteria"].as_object().unwrap();
    assert!(by_criteria.contains_key("relevance"));
    assert!(by_criteria.contains_key("helpfulness"));
    assert!(by_criteria.contains_key("tone"));

    // Each criterion score should be between 0 and 1.
    for (key, value) in by_criteria {
        let score = value.as_f64().unwrap();
        assert!(
            (0.0..=1.0).contains(&score),
            "Score for '{key}' should be between 0.0 and 1.0, got {score}"
        );
    }

    // Suggestions should be an array.
    assert!(body["suggestions"].is_array());
}

// ---------------------------------------------------------------------------
// 7. POST /api/v1/agent/personas creates a persona, GET retrieves it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_persona_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a persona.
    let create_resp = client
        .post(format!("{base_url}/api/v1/agent/personas"))
        .json(&serde_json::json!({
            "tenant_id": "tenant-abc",
            "agent_role": "sales_qualifier",
            "persona": {
                "name": "Ana",
                "tone": "friendly",
                "language_style": "es_latam",
                "signature": "-- Ana, Xcapit Sales",
                "custom_instructions": "Always greet the customer by name."
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(create_resp.status(), 201);

    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    assert_eq!(create_body["created"], true);
    assert_eq!(create_body["tenant_id"], "tenant-abc");
    assert_eq!(create_body["agent_role"], "sales_qualifier");
    assert_eq!(create_body["persona_name"], "Ana");

    // List personas for the tenant.
    let list_resp = client
        .get(format!("{base_url}/api/v1/agent/personas/tenant-abc"))
        .send()
        .await
        .unwrap();

    assert_eq!(list_resp.status(), 200);

    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    assert_eq!(list_body["tenant_id"], "tenant-abc");

    let personas = list_body["personas"].as_object().unwrap();
    assert_eq!(personas.len(), 1);
    assert!(personas.contains_key("sales_qualifier"));

    let persona = &personas["sales_qualifier"];
    assert_eq!(persona["name"], "Ana");
    assert_eq!(persona["tone"], "friendly");
    assert_eq!(persona["language_style"], "es_latam");
}

// ---------------------------------------------------------------------------
// 8. POST /api/v1/tenants/{id}/register creates tenant, GET status shows it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tenant_registration_and_status() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    // Register a new tenant on the "pro" plan.
    let register_resp = client
        .post(format!("{base_url}/api/v1/tenants/tenant-xyz/register"))
        .json(&serde_json::json!({
            "plan": "pro"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(register_resp.status(), 201);

    let register_body: serde_json::Value = register_resp.json().await.unwrap();
    assert_eq!(register_body["tenant_id"], "tenant-xyz");
    assert_eq!(register_body["plan"], "pro");
    assert_eq!(register_body["status"], "active");

    // Get tenant status.
    let status_resp = client
        .get(format!("{base_url}/api/v1/tenants/tenant-xyz/status"))
        .send()
        .await
        .unwrap();

    assert_eq!(status_resp.status(), 200);

    let status_body: serde_json::Value = status_resp.json().await.unwrap();
    assert_eq!(status_body["tenant_id"], "tenant-xyz");
    assert!(status_body["limits"].is_object(), "Expected limits object");
    assert!(status_body["usage"].is_object(), "Expected usage object");
}

// ---------------------------------------------------------------------------
// 9. GET /api/v1/usage/tenant/{id} returns empty usage for new tenant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_usage_empty_tenant() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base_url}/api/v1/usage/tenant/brand-new-tenant"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    // For a tenant with no usage, expect zero counts.
    assert_eq!(body["request_count"], 0);
    assert_eq!(body["total_tokens_in"], 0);
    assert_eq!(body["total_tokens_out"], 0);
}

// ---------------------------------------------------------------------------
// 10. POST /api/v1/proxy/webhook without secret returns 401 or 403
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_webhook_proxy_no_secret() {
    // Create a server with a non-empty HMAC secret so secret validation is enforced.
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(AuditLog::new(tmp.path().join("audit")));
    let skills = Arc::new(SkillRegistry::new());
    let permissions = PermissionSet::new();
    let config = XcapitConfig {
        webhook_hmac_secret: "super-secret-hmac-key".to_string(),
        ..XcapitConfig::default()
    };

    let state = Arc::new(XcapitState::new(skills, permissions, audit, config));
    let app = xcapitsff_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    // Send a webhook from an allowed source but without the x-webhook-secret header.
    let resp = client
        .post(format!("{base_url}/api/v1/proxy/webhook"))
        .json(&serde_json::json!({
            "event": "lead.created",
            "data": {"name": "John"},
            "source": "hubspot"
        }))
        .send()
        .await
        .unwrap();

    // Should be 401 (missing HMAC).
    assert_eq!(
        resp.status().as_u16(),
        401,
        "Missing secret should result in 401"
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["forwarded"], false);
}

// ---------------------------------------------------------------------------
// 11. POST /api/v1/proxy/webhook with unknown source returns 403
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_webhook_proxy_unknown_source() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base_url}/api/v1/proxy/webhook"))
        .json(&serde_json::json!({
            "event": "lead.created",
            "data": {"name": "Jane"},
            "source": "unknown_system_xyz"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Unknown source should result in 403"
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["forwarded"], false);
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("not in allowed list"),
        "Error should mention source not allowed, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// 12. GET /api/v1/tenants/{id}/status for unregistered tenant returns 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_tenant_status_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{base_url}/api/v1/tenants/nonexistent-tenant/status"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);

    let body: serde_json::Value = resp.json().await.unwrap();
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("not registered"),
        "Error should mention tenant not registered, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// 13. POST /api/v1/agent/evaluate with default criteria returns 4 criteria
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_evaluate_default_criteria() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base_url}/api/v1/agent/evaluate"))
        .json(&serde_json::json!({
            "context": "Explain Rust ownership",
            "response": "Rust ownership is a set of rules governing memory management. Each value has a single owner, and when the owner goes out of scope, the value is dropped."
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let by_criteria = body["by_criteria"].as_object().unwrap();

    // When no criteria are specified, defaults to: relevance, helpfulness, accuracy, tone.
    assert_eq!(by_criteria.len(), 4);
    assert!(by_criteria.contains_key("relevance"));
    assert!(by_criteria.contains_key("helpfulness"));
    assert!(by_criteria.contains_key("accuracy"));
    assert!(by_criteria.contains_key("tone"));
}

// ---------------------------------------------------------------------------
// 14. Multiple personas for the same tenant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_personas_per_tenant() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create persona for sales_qualifier.
    client
        .post(format!("{base_url}/api/v1/agent/personas"))
        .json(&serde_json::json!({
            "tenant_id": "tenant-multi",
            "agent_role": "sales_qualifier",
            "persona": {
                "name": "Carlos",
                "tone": "professional",
                "language_style": "es_latam"
            }
        }))
        .send()
        .await
        .unwrap();

    // Create persona for support_responder.
    client
        .post(format!("{base_url}/api/v1/agent/personas"))
        .json(&serde_json::json!({
            "tenant_id": "tenant-multi",
            "agent_role": "support_responder",
            "persona": {
                "name": "Maria",
                "tone": "empathetic",
                "language_style": "es_latam"
            }
        }))
        .send()
        .await
        .unwrap();

    // List should return both personas.
    let list_resp = client
        .get(format!("{base_url}/api/v1/agent/personas/tenant-multi"))
        .send()
        .await
        .unwrap();

    assert_eq!(list_resp.status(), 200);

    let body: serde_json::Value = list_resp.json().await.unwrap();
    let personas = body["personas"].as_object().unwrap();
    assert_eq!(personas.len(), 2);
    assert_eq!(personas["sales_qualifier"]["name"], "Carlos");
    assert_eq!(personas["support_responder"]["name"], "Maria");
}

// ---------------------------------------------------------------------------
// 15. Empty personas for unknown tenant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_personas_empty_tenant() {
    let (base_url, _tmp) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base_url}/api/v1/agent/personas/unknown-tenant"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["tenant_id"], "unknown-tenant");
    let personas = body["personas"].as_object().unwrap();
    assert!(personas.is_empty());
}
