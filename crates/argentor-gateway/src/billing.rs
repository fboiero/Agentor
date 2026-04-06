//! Billing integration with webhook endpoints and plan enforcement.
//!
//! Provides a [`BillingManager`] that manages subscriptions, generates invoices,
//! and processes Stripe-compatible webhooks. REST endpoints are mounted under
//! `/api/v1/billing/` via [`billing_router`].
//!
//! # Endpoints
//!
//! - `POST   /api/v1/billing/subscriptions`                    — Create subscription
//! - `GET    /api/v1/billing/subscriptions/{tenant_id}`        — Get subscription
//! - `PUT    /api/v1/billing/subscriptions/{tenant_id}/plan`   — Upgrade plan
//! - `DELETE /api/v1/billing/subscriptions/{tenant_id}`        — Cancel subscription
//! - `GET    /api/v1/billing/invoices/{tenant_id}`             — List invoices
//! - `GET    /api/v1/billing/invoices/{tenant_id}/{invoice_id}` — Get single invoice
//! - `POST   /api/v1/billing/webhooks/stripe`                  — Stripe webhook receiver

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Billing Plan
// ---------------------------------------------------------------------------

/// Billing plan with pricing and included allowances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum BillingPlan {
    /// Free tier — no monthly cost.
    Free,
    /// Professional tier.
    Pro,
    /// Enterprise tier.
    Enterprise,
    /// Custom plan with arbitrary pricing.
    Custom {
        /// Monthly price in USD.
        monthly_price_usd: f64,
        /// Plan display name.
        name: String,
    },
}

impl BillingPlan {
    /// Monthly price in USD.
    pub fn monthly_price_usd(&self) -> f64 {
        match self {
            Self::Free => 0.0,
            Self::Pro => 49.0,
            Self::Enterprise => 499.0,
            Self::Custom {
                monthly_price_usd, ..
            } => *monthly_price_usd,
        }
    }

    /// Tokens included in this plan per billing period.
    pub fn included_tokens(&self) -> u64 {
        match self {
            Self::Free => 50_000,
            Self::Pro => 2_000_000,
            Self::Enterprise => 50_000_000,
            Self::Custom { .. } => 10_000_000,
        }
    }

    /// Requests included in this plan per billing period.
    pub fn included_requests(&self) -> u64 {
        match self {
            Self::Free => 100,
            Self::Pro => 5_000,
            Self::Enterprise => 100_000,
            Self::Custom { .. } => 25_000,
        }
    }

    /// Overage price per 1 000 tokens above the included allowance.
    pub fn overage_price_per_1k_tokens(&self) -> f64 {
        match self {
            Self::Free => 0.0, // no overage allowed
            Self::Pro => 0.005,
            Self::Enterprise => 0.003,
            Self::Custom { .. } => 0.004,
        }
    }

    /// Human-readable plan label.
    pub fn label(&self) -> &str {
        match self {
            Self::Free => "Free",
            Self::Pro => "Pro",
            Self::Enterprise => "Enterprise",
            Self::Custom { name, .. } => name.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

/// Status of a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    /// Active and billing normally.
    Active,
    /// Payment failed; grace period.
    PastDue,
    /// Canceled by the tenant or admin.
    Canceled,
    /// In a trial period.
    Trialing,
}

/// A tenant subscription record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Unique subscription identifier.
    pub id: String,
    /// Tenant that owns this subscription.
    pub tenant_id: String,
    /// Active billing plan.
    pub plan: BillingPlan,
    /// Current subscription status.
    pub status: SubscriptionStatus,
    /// Start of the current billing period.
    pub current_period_start: DateTime<Utc>,
    /// End of the current billing period.
    pub current_period_end: DateTime<Utc>,
    /// Masked payment method (last 4 digits), if set.
    pub payment_method: Option<String>,
    /// When the subscription was created.
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Invoice
// ---------------------------------------------------------------------------

/// Status of an invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvoiceStatus {
    /// Invoice is being prepared.
    Draft,
    /// Invoice sent / awaiting payment.
    Open,
    /// Invoice has been paid.
    Paid,
    /// Invoice was voided.
    Void,
}

/// A single line item on an invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    /// Description of the charge.
    pub description: String,
    /// Quantity billed.
    pub quantity: u64,
    /// Unit price in USD.
    pub unit_price_usd: f64,
    /// Total amount for this line item in USD.
    pub amount_usd: f64,
}

/// An invoice for a billing period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    /// Unique invoice identifier.
    pub id: String,
    /// Tenant that owns this invoice.
    pub tenant_id: String,
    /// Billing period label (e.g. "2026-04").
    pub period: String,
    /// Base plan amount in USD.
    pub base_amount_usd: f64,
    /// Overage charges in USD.
    pub overage_amount_usd: f64,
    /// Total amount in USD (base + overage).
    pub total_amount_usd: f64,
    /// Detailed line items.
    pub line_items: Vec<InvoiceLineItem>,
    /// Invoice status.
    pub status: InvoiceStatus,
    /// When the invoice was created.
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------------

/// Result of processing a webhook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookProcessResult {
    /// Whether the event was processed successfully.
    pub success: bool,
    /// Event type that was processed.
    pub event_type: String,
    /// Optional message with processing details.
    pub message: String,
}

// ---------------------------------------------------------------------------
// BillingManager
// ---------------------------------------------------------------------------

/// Inner mutable state for the billing manager.
struct BillingInner {
    /// Active subscriptions indexed by tenant ID.
    subscriptions: HashMap<String, Subscription>,
    /// Invoices indexed by tenant ID.
    invoices: HashMap<String, Vec<Invoice>>,
    /// Simulated usage counters: (tokens_used, requests_used) per (tenant, period).
    usage: HashMap<(String, String), (u64, u64)>,
}

/// Thread-safe billing manager that handles subscriptions, invoices, and webhooks.
pub struct BillingManager {
    inner: Arc<RwLock<BillingInner>>,
    /// Stripe webhook signing secret for HMAC verification.
    webhook_secret: String,
}

impl BillingManager {
    /// Create a new billing manager.
    pub fn new(webhook_secret: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(BillingInner {
                subscriptions: HashMap::new(),
                invoices: HashMap::new(),
                usage: HashMap::new(),
            })),
            webhook_secret,
        }
    }

    /// Create a subscription for a tenant.
    pub async fn create_subscription(
        &self,
        tenant_id: &str,
        plan: BillingPlan,
        payment_method: Option<String>,
    ) -> Subscription {
        let now = Utc::now();
        let period_end = now + chrono::Duration::days(30);
        let sub = Subscription {
            id: format!("sub_{}", Uuid::new_v4()),
            tenant_id: tenant_id.to_string(),
            plan,
            status: SubscriptionStatus::Active,
            current_period_start: now,
            current_period_end: period_end,
            payment_method: payment_method.map(|pm| mask_payment_method(&pm)),
            created_at: now,
        };
        let mut inner = self.inner.write().await;
        inner
            .subscriptions
            .insert(tenant_id.to_string(), sub.clone());
        sub
    }

    /// Cancel a tenant's subscription. Returns `true` if it existed.
    pub async fn cancel_subscription(&self, tenant_id: &str) -> bool {
        let mut inner = self.inner.write().await;
        if let Some(sub) = inner.subscriptions.get_mut(tenant_id) {
            sub.status = SubscriptionStatus::Canceled;
            true
        } else {
            false
        }
    }

    /// Upgrade (or change) a tenant's plan. Returns the updated subscription.
    pub async fn upgrade_plan(
        &self,
        tenant_id: &str,
        new_plan: BillingPlan,
    ) -> Option<Subscription> {
        let mut inner = self.inner.write().await;
        if let Some(sub) = inner.subscriptions.get_mut(tenant_id) {
            sub.plan = new_plan;
            sub.status = SubscriptionStatus::Active;
            Some(sub.clone())
        } else {
            None
        }
    }

    /// Get a tenant's subscription.
    pub async fn get_subscription(&self, tenant_id: &str) -> Option<Subscription> {
        let inner = self.inner.read().await;
        inner.subscriptions.get(tenant_id).cloned()
    }

    /// Record usage for a tenant in a given period.
    pub async fn record_usage(&self, tenant_id: &str, period: &str, tokens: u64, requests: u64) {
        let mut inner = self.inner.write().await;
        let key = (tenant_id.to_string(), period.to_string());
        let entry = inner.usage.entry(key).or_insert((0, 0));
        entry.0 += tokens;
        entry.1 += requests;
    }

    /// Generate an invoice for a tenant for a given period.
    pub async fn generate_invoice(&self, tenant_id: &str, period: &str) -> Option<Invoice> {
        let mut inner = self.inner.write().await;
        let sub = inner.subscriptions.get(tenant_id)?;
        let plan = sub.plan.clone();

        let usage_key = (tenant_id.to_string(), period.to_string());
        let (tokens_used, requests_used) = inner.usage.get(&usage_key).copied().unwrap_or((0, 0));

        let base_amount_usd = plan.monthly_price_usd();

        // Calculate overage
        let mut line_items = vec![InvoiceLineItem {
            description: format!("{} plan — base fee", plan.label()),
            quantity: 1,
            unit_price_usd: base_amount_usd,
            amount_usd: base_amount_usd,
        }];

        let overage_tokens = tokens_used.saturating_sub(plan.included_tokens());
        let overage_amount_usd = if overage_tokens > 0 {
            let cost = (overage_tokens as f64 / 1000.0) * plan.overage_price_per_1k_tokens();
            line_items.push(InvoiceLineItem {
                description: format!("Token overage ({overage_tokens} tokens)"),
                quantity: overage_tokens,
                unit_price_usd: plan.overage_price_per_1k_tokens() / 1000.0,
                amount_usd: cost,
            });
            cost
        } else {
            0.0
        };

        let overage_requests = requests_used.saturating_sub(plan.included_requests());
        let request_overage_usd = if overage_requests > 0 && plan != BillingPlan::Free {
            let cost = overage_requests as f64 * 0.001; // $0.001 per extra request
            line_items.push(InvoiceLineItem {
                description: format!("Request overage ({overage_requests} requests)"),
                quantity: overage_requests,
                unit_price_usd: 0.001,
                amount_usd: cost,
            });
            cost
        } else {
            0.0
        };

        let total_overage = overage_amount_usd + request_overage_usd;
        let total_amount_usd = base_amount_usd + total_overage;

        let invoice = Invoice {
            id: format!("inv_{}", Uuid::new_v4()),
            tenant_id: tenant_id.to_string(),
            period: period.to_string(),
            base_amount_usd,
            overage_amount_usd: total_overage,
            total_amount_usd,
            line_items,
            status: InvoiceStatus::Open,
            created_at: Utc::now(),
        };

        inner
            .invoices
            .entry(tenant_id.to_string())
            .or_default()
            .push(invoice.clone());

        Some(invoice)
    }

    /// List all invoices for a tenant.
    pub async fn list_invoices(&self, tenant_id: &str) -> Vec<Invoice> {
        let inner = self.inner.read().await;
        inner.invoices.get(tenant_id).cloned().unwrap_or_default()
    }

    /// Get a specific invoice by ID.
    pub async fn get_invoice(&self, tenant_id: &str, invoice_id: &str) -> Option<Invoice> {
        let inner = self.inner.read().await;
        inner
            .invoices
            .get(tenant_id)?
            .iter()
            .find(|inv| inv.id == invoice_id)
            .cloned()
    }

    /// Process a Stripe-compatible webhook event.
    pub async fn process_webhook(
        &self,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> WebhookProcessResult {
        match event_type {
            "customer.subscription.created" => {
                let tenant_id = payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let plan = parse_plan_from_payload(payload);
                let payment_method = payload
                    .get("payment_method")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string);
                self.create_subscription(tenant_id, plan, payment_method)
                    .await;
                WebhookProcessResult {
                    success: true,
                    event_type: event_type.to_string(),
                    message: format!("Subscription created for tenant {tenant_id}"),
                }
            }
            "customer.subscription.updated" => {
                let tenant_id = payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let plan = parse_plan_from_payload(payload);
                let upgraded = self.upgrade_plan(tenant_id, plan).await;
                WebhookProcessResult {
                    success: upgraded.is_some(),
                    event_type: event_type.to_string(),
                    message: if upgraded.is_some() {
                        format!("Subscription updated for tenant {tenant_id}")
                    } else {
                        format!("No subscription found for tenant {tenant_id}")
                    },
                }
            }
            "customer.subscription.deleted" => {
                let tenant_id = payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let canceled = self.cancel_subscription(tenant_id).await;
                WebhookProcessResult {
                    success: canceled,
                    event_type: event_type.to_string(),
                    message: if canceled {
                        format!("Subscription canceled for tenant {tenant_id}")
                    } else {
                        format!("No subscription found for tenant {tenant_id}")
                    },
                }
            }
            "invoice.paid" => {
                let tenant_id = payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let invoice_id = payload
                    .get("invoice_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let marked = self.mark_invoice_paid(tenant_id, invoice_id).await;
                WebhookProcessResult {
                    success: marked,
                    event_type: event_type.to_string(),
                    message: if marked {
                        format!("Invoice {invoice_id} marked as paid")
                    } else {
                        format!("Invoice {invoice_id} not found for tenant {tenant_id}")
                    },
                }
            }
            "invoice.payment_failed" => {
                let tenant_id = payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                self.mark_subscription_past_due(tenant_id).await;
                WebhookProcessResult {
                    success: true,
                    event_type: event_type.to_string(),
                    message: format!("Subscription marked past due for tenant {tenant_id}"),
                }
            }
            _ => WebhookProcessResult {
                success: false,
                event_type: event_type.to_string(),
                message: format!("Unhandled event type: {event_type}"),
            },
        }
    }

    /// Mark an invoice as paid.
    async fn mark_invoice_paid(&self, tenant_id: &str, invoice_id: &str) -> bool {
        let mut inner = self.inner.write().await;
        if let Some(invoices) = inner.invoices.get_mut(tenant_id) {
            if let Some(inv) = invoices.iter_mut().find(|i| i.id == invoice_id) {
                inv.status = InvoiceStatus::Paid;
                return true;
            }
        }
        false
    }

    /// Mark a tenant's subscription as past due.
    async fn mark_subscription_past_due(&self, tenant_id: &str) {
        let mut inner = self.inner.write().await;
        if let Some(sub) = inner.subscriptions.get_mut(tenant_id) {
            sub.status = SubscriptionStatus::PastDue;
        }
    }

    /// Verify the HMAC-SHA256 signature of a Stripe webhook payload.
    pub fn verify_webhook_signature(&self, payload: &str, signature: &str) -> bool {
        if self.webhook_secret.is_empty() {
            // No secret configured — skip verification.
            return true;
        }
        let expected = compute_hmac_sha256(&self.webhook_secret, payload);
        constant_time_eq(&expected, signature)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Mask a payment method string to show only the last 4 digits.
fn mask_payment_method(pm: &str) -> String {
    if pm.len() >= 4 {
        format!("****{}", &pm[pm.len() - 4..])
    } else {
        "****".to_string()
    }
}

/// Parse a [`BillingPlan`] from a webhook payload JSON object.
fn parse_plan_from_payload(payload: &serde_json::Value) -> BillingPlan {
    match payload.get("plan").and_then(|v| v.as_str()) {
        Some("free") | Some("Free") => BillingPlan::Free,
        Some("pro") | Some("Pro") => BillingPlan::Pro,
        Some("enterprise") | Some("Enterprise") => BillingPlan::Enterprise,
        Some(name) => {
            let price = payload
                .get("monthly_price_usd")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            BillingPlan::Custom {
                monthly_price_usd: price,
                name: name.to_string(),
            }
        }
        None => BillingPlan::Free,
    }
}

/// Compute HMAC-SHA256 using the same concatenation approach as the rest of the crate.
fn compute_hmac_sha256(secret: &str, payload: &str) -> String {
    let block_size = 64usize;
    let mut key_padded = vec![0u8; block_size];

    let key = secret.as_bytes();
    if key.len() > block_size {
        let mut hasher = sha2::Sha256::new();
        hasher.update(key);
        let hashed = hasher.finalize();
        key_padded[..32].copy_from_slice(&hashed);
    } else {
        key_padded[..key.len()].copy_from_slice(key);
    }

    let mut ipad = vec![0x36u8; block_size];
    let mut opad = vec![0x5cu8; block_size];
    for i in 0..block_size {
        ipad[i] ^= key_padded[i];
        opad[i] ^= key_padded[i];
    }

    let mut inner_hasher = sha2::Sha256::new();
    inner_hasher.update(&ipad);
    inner_hasher.update(payload.as_bytes());
    let inner_hash = inner_hasher.finalize();

    let mut outer_hasher = sha2::Sha256::new();
    outer_hasher.update(&opad);
    outer_hasher.update(inner_hash);
    let result = outer_hasher.finalize();

    hex::encode(result)
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Return the current billing period label (e.g. "2026-04").
#[allow(dead_code)]
fn current_period_label() -> String {
    let now = Utc::now();
    format!("{:04}-{:02}", now.year(), now.month())
}

// ---------------------------------------------------------------------------
// REST API — State & Router
// ---------------------------------------------------------------------------

/// Shared state for billing endpoints.
pub struct BillingState {
    /// The billing manager.
    pub manager: BillingManager,
}

/// Create the billing Axum router with all endpoints.
pub fn billing_router(state: Arc<BillingState>) -> Router {
    Router::new()
        .route(
            "/api/v1/billing/subscriptions",
            post(create_subscription_handler),
        )
        .route(
            "/api/v1/billing/subscriptions/{tenant_id}",
            get(get_subscription_handler).delete(cancel_subscription_handler),
        )
        .route(
            "/api/v1/billing/subscriptions/{tenant_id}/plan",
            put(upgrade_plan_handler),
        )
        .route(
            "/api/v1/billing/invoices/{tenant_id}",
            get(list_invoices_handler),
        )
        .route(
            "/api/v1/billing/invoices/{tenant_id}/{invoice_id}",
            get(get_invoice_handler),
        )
        .route(
            "/api/v1/billing/webhooks/stripe",
            post(stripe_webhook_handler),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / Response bodies
// ---------------------------------------------------------------------------

/// Request body for POST /api/v1/billing/subscriptions.
#[derive(Debug, Deserialize)]
struct CreateSubscriptionRequest {
    tenant_id: String,
    plan: BillingPlan,
    payment_method: Option<String>,
}

/// Request body for PUT /api/v1/billing/subscriptions/{tenant_id}/plan.
#[derive(Debug, Deserialize)]
struct UpgradePlanRequest {
    plan: BillingPlan,
}

/// Request body for POST /api/v1/billing/webhooks/stripe.
#[derive(Debug, Deserialize)]
struct StripeWebhookBody {
    #[serde(rename = "type")]
    event_type: String,
    data: serde_json::Value,
}

/// Generic error response.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/billing/subscriptions
async fn create_subscription_handler(
    State(state): State<Arc<BillingState>>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> impl IntoResponse {
    let sub = state
        .manager
        .create_subscription(&req.tenant_id, req.plan, req.payment_method)
        .await;
    (StatusCode::CREATED, Json(sub)).into_response()
}

/// GET /api/v1/billing/subscriptions/{tenant_id}
async fn get_subscription_handler(
    State(state): State<Arc<BillingState>>,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    match state.manager.get_subscription(&tenant_id).await {
        Some(sub) => Json(sub).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("No subscription found for tenant {tenant_id}"),
            }),
        )
            .into_response(),
    }
}

/// PUT /api/v1/billing/subscriptions/{tenant_id}/plan
async fn upgrade_plan_handler(
    State(state): State<Arc<BillingState>>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpgradePlanRequest>,
) -> impl IntoResponse {
    match state.manager.upgrade_plan(&tenant_id, req.plan).await {
        Some(sub) => Json(sub).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("No subscription found for tenant {tenant_id}"),
            }),
        )
            .into_response(),
    }
}

/// DELETE /api/v1/billing/subscriptions/{tenant_id}
async fn cancel_subscription_handler(
    State(state): State<Arc<BillingState>>,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let canceled = state.manager.cancel_subscription(&tenant_id).await;
    if canceled {
        (
            StatusCode::OK,
            Json(serde_json::json!({"canceled": true, "tenant_id": tenant_id})),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("No subscription found for tenant {tenant_id}"),
            }),
        )
            .into_response()
    }
}

/// GET /api/v1/billing/invoices/{tenant_id}
async fn list_invoices_handler(
    State(state): State<Arc<BillingState>>,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let invoices = state.manager.list_invoices(&tenant_id).await;
    Json(invoices).into_response()
}

/// GET /api/v1/billing/invoices/{tenant_id}/{invoice_id}
async fn get_invoice_handler(
    State(state): State<Arc<BillingState>>,
    Path((tenant_id, invoice_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.manager.get_invoice(&tenant_id, &invoice_id).await {
        Some(inv) => Json(inv).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Invoice {invoice_id} not found for tenant {tenant_id}"),
            }),
        )
            .into_response(),
    }
}

/// POST /api/v1/billing/webhooks/stripe
///
/// Verifies the `Stripe-Signature` header against the payload before processing.
async fn stripe_webhook_handler(
    State(state): State<Arc<BillingState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Verify signature
    let signature = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !state.manager.verify_webhook_signature(&body, signature) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid webhook signature".to_string(),
            }),
        )
            .into_response();
    }

    // Parse body
    let webhook_body: StripeWebhookBody = match serde_json::from_str(&body) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid webhook body: {e}"),
                }),
            )
                .into_response();
        }
    };

    let result = state
        .manager
        .process_webhook(&webhook_body.event_type, &webhook_body.data)
        .await;

    if result.success {
        Json(result).into_response()
    } else {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(result)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    // -- Helpers -----------------------------------------------------------

    fn make_state() -> Arc<BillingState> {
        Arc::new(BillingState {
            manager: BillingManager::new("test_webhook_secret".to_string()),
        })
    }

    fn make_state_no_secret() -> Arc<BillingState> {
        Arc::new(BillingState {
            manager: BillingManager::new(String::new()),
        })
    }

    fn sign_payload(secret: &str, payload: &str) -> String {
        compute_hmac_sha256(secret, payload)
    }

    // -- BillingPlan tests -------------------------------------------------

    #[test]
    fn test_free_plan_pricing() {
        let plan = BillingPlan::Free;
        assert_eq!(plan.monthly_price_usd(), 0.0);
        assert_eq!(plan.included_tokens(), 50_000);
        assert_eq!(plan.included_requests(), 100);
        assert_eq!(plan.overage_price_per_1k_tokens(), 0.0);
        assert_eq!(plan.label(), "Free");
    }

    #[test]
    fn test_pro_plan_pricing() {
        let plan = BillingPlan::Pro;
        assert_eq!(plan.monthly_price_usd(), 49.0);
        assert_eq!(plan.included_tokens(), 2_000_000);
        assert_eq!(plan.included_requests(), 5_000);
        assert_eq!(plan.overage_price_per_1k_tokens(), 0.005);
        assert_eq!(plan.label(), "Pro");
    }

    #[test]
    fn test_enterprise_plan_pricing() {
        let plan = BillingPlan::Enterprise;
        assert_eq!(plan.monthly_price_usd(), 499.0);
        assert_eq!(plan.included_tokens(), 50_000_000);
        assert_eq!(plan.included_requests(), 100_000);
        assert_eq!(plan.overage_price_per_1k_tokens(), 0.003);
        assert_eq!(plan.label(), "Enterprise");
    }

    #[test]
    fn test_custom_plan_pricing() {
        let plan = BillingPlan::Custom {
            monthly_price_usd: 199.0,
            name: "Startup".to_string(),
        };
        assert_eq!(plan.monthly_price_usd(), 199.0);
        assert_eq!(plan.included_tokens(), 10_000_000);
        assert_eq!(plan.label(), "Startup");
    }

    // -- Subscription tests ------------------------------------------------

    #[tokio::test]
    async fn test_create_subscription() {
        let mgr = BillingManager::new(String::new());
        let sub = mgr
            .create_subscription("t1", BillingPlan::Pro, Some("4242424242424242".to_string()))
            .await;
        assert!(sub.id.starts_with("sub_"));
        assert_eq!(sub.tenant_id, "t1");
        assert_eq!(sub.plan, BillingPlan::Pro);
        assert_eq!(sub.status, SubscriptionStatus::Active);
        assert_eq!(sub.payment_method, Some("****4242".to_string()));
    }

    #[tokio::test]
    async fn test_create_subscription_no_payment() {
        let mgr = BillingManager::new(String::new());
        let sub = mgr.create_subscription("t1", BillingPlan::Free, None).await;
        assert!(sub.payment_method.is_none());
        assert_eq!(sub.plan, BillingPlan::Free);
    }

    #[tokio::test]
    async fn test_get_subscription() {
        let mgr = BillingManager::new(String::new());
        assert!(mgr.get_subscription("t1").await.is_none());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let sub = mgr.get_subscription("t1").await;
        assert!(sub.is_some());
        assert_eq!(sub.unwrap().plan, BillingPlan::Pro);
    }

    #[tokio::test]
    async fn test_cancel_subscription() {
        let mgr = BillingManager::new(String::new());
        assert!(!mgr.cancel_subscription("t1").await);
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        assert!(mgr.cancel_subscription("t1").await);
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Canceled);
    }

    #[tokio::test]
    async fn test_upgrade_plan() {
        let mgr = BillingManager::new(String::new());
        assert!(mgr
            .upgrade_plan("t1", BillingPlan::Enterprise)
            .await
            .is_none());
        mgr.create_subscription("t1", BillingPlan::Free, None).await;
        let updated = mgr.upgrade_plan("t1", BillingPlan::Enterprise).await;
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().plan, BillingPlan::Enterprise);
    }

    #[tokio::test]
    async fn test_upgrade_reactivates_canceled() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        mgr.cancel_subscription("t1").await;
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Canceled);
        let updated = mgr
            .upgrade_plan("t1", BillingPlan::Enterprise)
            .await
            .unwrap();
        assert_eq!(updated.status, SubscriptionStatus::Active);
    }

    // -- Invoice tests -----------------------------------------------------

    #[tokio::test]
    async fn test_generate_invoice_basic() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        assert!(inv.id.starts_with("inv_"));
        assert_eq!(inv.tenant_id, "t1");
        assert_eq!(inv.period, "2026-04");
        assert_eq!(inv.base_amount_usd, 49.0);
        assert_eq!(inv.overage_amount_usd, 0.0);
        assert_eq!(inv.total_amount_usd, 49.0);
        assert_eq!(inv.status, InvoiceStatus::Open);
        assert_eq!(inv.line_items.len(), 1);
    }

    #[tokio::test]
    async fn test_generate_invoice_no_subscription() {
        let mgr = BillingManager::new(String::new());
        assert!(mgr.generate_invoice("t1", "2026-04").await.is_none());
    }

    #[tokio::test]
    async fn test_generate_invoice_with_token_overage() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        // Pro includes 2M tokens; record 2.5M
        mgr.record_usage("t1", "2026-04", 2_500_000, 100).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        assert_eq!(inv.base_amount_usd, 49.0);
        // 500k overage * 0.005/1k = 2.5
        assert!((inv.overage_amount_usd - 2.5).abs() < 0.01);
        assert_eq!(inv.line_items.len(), 2); // base + token overage
    }

    #[tokio::test]
    async fn test_generate_invoice_with_request_overage() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        // Pro includes 5000 requests; record 6000
        mgr.record_usage("t1", "2026-04", 100, 6_000).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        // 1000 overage requests * $0.001 = $1.0
        assert!((inv.overage_amount_usd - 1.0).abs() < 0.01);
        assert_eq!(inv.line_items.len(), 2); // base + request overage
    }

    #[tokio::test]
    async fn test_generate_invoice_with_both_overages() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        mgr.record_usage("t1", "2026-04", 3_000_000, 7_000).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        // Token overage: 1M * 0.005/1k = 5.0
        // Request overage: 2000 * 0.001 = 2.0
        assert!((inv.overage_amount_usd - 7.0).abs() < 0.01);
        assert_eq!(inv.line_items.len(), 3);
    }

    #[tokio::test]
    async fn test_list_invoices() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        mgr.generate_invoice("t1", "2026-03").await;
        mgr.generate_invoice("t1", "2026-04").await;
        let invoices = mgr.list_invoices("t1").await;
        assert_eq!(invoices.len(), 2);
    }

    #[tokio::test]
    async fn test_list_invoices_empty() {
        let mgr = BillingManager::new(String::new());
        let invoices = mgr.list_invoices("t1").await;
        assert!(invoices.is_empty());
    }

    #[tokio::test]
    async fn test_get_invoice_by_id() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        let fetched = mgr.get_invoice("t1", &inv.id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, inv.id);
    }

    #[tokio::test]
    async fn test_get_invoice_not_found() {
        let mgr = BillingManager::new(String::new());
        assert!(mgr.get_invoice("t1", "inv_nonexistent").await.is_none());
    }

    // -- Webhook tests -----------------------------------------------------

    #[tokio::test]
    async fn test_webhook_subscription_created() {
        let mgr = BillingManager::new(String::new());
        let payload = serde_json::json!({
            "tenant_id": "t1",
            "plan": "pro",
            "payment_method": "4242"
        });
        let result = mgr
            .process_webhook("customer.subscription.created", &payload)
            .await;
        assert!(result.success);
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.plan, BillingPlan::Pro);
    }

    #[tokio::test]
    async fn test_webhook_subscription_updated() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Free, None).await;
        let payload = serde_json::json!({
            "tenant_id": "t1",
            "plan": "enterprise"
        });
        let result = mgr
            .process_webhook("customer.subscription.updated", &payload)
            .await;
        assert!(result.success);
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.plan, BillingPlan::Enterprise);
    }

    #[tokio::test]
    async fn test_webhook_subscription_deleted() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let payload = serde_json::json!({"tenant_id": "t1"});
        let result = mgr
            .process_webhook("customer.subscription.deleted", &payload)
            .await;
        assert!(result.success);
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Canceled);
    }

    #[tokio::test]
    async fn test_webhook_invoice_paid() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        let payload = serde_json::json!({
            "tenant_id": "t1",
            "invoice_id": inv.id
        });
        let result = mgr.process_webhook("invoice.paid", &payload).await;
        assert!(result.success);
        let fetched = mgr.get_invoice("t1", &inv.id).await.unwrap();
        assert_eq!(fetched.status, InvoiceStatus::Paid);
    }

    #[tokio::test]
    async fn test_webhook_payment_failed() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Pro, None).await;
        let payload = serde_json::json!({"tenant_id": "t1"});
        let result = mgr
            .process_webhook("invoice.payment_failed", &payload)
            .await;
        assert!(result.success);
        let sub = mgr.get_subscription("t1").await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::PastDue);
    }

    #[tokio::test]
    async fn test_webhook_unknown_event() {
        let mgr = BillingManager::new(String::new());
        let result = mgr
            .process_webhook("unknown.event", &serde_json::json!({}))
            .await;
        assert!(!result.success);
        assert!(result.message.contains("Unhandled"));
    }

    // -- HMAC signature tests ----------------------------------------------

    #[test]
    fn test_hmac_signature_valid() {
        let mgr = BillingManager::new("secret123".to_string());
        let payload = r#"{"type":"invoice.paid","data":{}}"#;
        let sig = compute_hmac_sha256("secret123", payload);
        assert!(mgr.verify_webhook_signature(payload, &sig));
    }

    #[test]
    fn test_hmac_signature_invalid() {
        let mgr = BillingManager::new("secret123".to_string());
        let payload = r#"{"type":"invoice.paid","data":{}}"#;
        assert!(!mgr.verify_webhook_signature(payload, "bad_signature_hex"));
    }

    #[test]
    fn test_hmac_signature_empty_secret_skips() {
        let mgr = BillingManager::new(String::new());
        assert!(mgr.verify_webhook_signature("anything", "anything"));
    }

    #[test]
    fn test_hmac_deterministic() {
        let sig1 = compute_hmac_sha256("key", "message");
        let sig2 = compute_hmac_sha256("key", "message");
        assert_eq!(sig1, sig2);
        // SHA256 HMAC output is 64 hex characters
        assert_eq!(sig1.len(), 64);
    }

    #[test]
    fn test_hmac_different_keys() {
        let sig1 = compute_hmac_sha256("key1", "message");
        let sig2 = compute_hmac_sha256("key2", "message");
        assert_ne!(sig1, sig2);
    }

    // -- Mask payment method tests -----------------------------------------

    #[test]
    fn test_mask_payment_method_long() {
        assert_eq!(mask_payment_method("4242424242424242"), "****4242");
    }

    #[test]
    fn test_mask_payment_method_short() {
        assert_eq!(mask_payment_method("42"), "****");
    }

    #[test]
    fn test_mask_payment_method_exact_4() {
        assert_eq!(mask_payment_method("1234"), "****1234");
    }

    // -- Parse plan tests --------------------------------------------------

    #[test]
    fn test_parse_plan_free() {
        let payload = serde_json::json!({"plan": "free"});
        assert_eq!(parse_plan_from_payload(&payload), BillingPlan::Free);
    }

    #[test]
    fn test_parse_plan_pro() {
        let payload = serde_json::json!({"plan": "Pro"});
        assert_eq!(parse_plan_from_payload(&payload), BillingPlan::Pro);
    }

    #[test]
    fn test_parse_plan_enterprise() {
        let payload = serde_json::json!({"plan": "enterprise"});
        assert_eq!(parse_plan_from_payload(&payload), BillingPlan::Enterprise);
    }

    #[test]
    fn test_parse_plan_custom() {
        let payload = serde_json::json!({"plan": "Startup", "monthly_price_usd": 99.0});
        let plan = parse_plan_from_payload(&payload);
        assert_eq!(
            plan,
            BillingPlan::Custom {
                monthly_price_usd: 99.0,
                name: "Startup".to_string()
            }
        );
    }

    #[test]
    fn test_parse_plan_missing_defaults_to_free() {
        let payload = serde_json::json!({});
        assert_eq!(parse_plan_from_payload(&payload), BillingPlan::Free);
    }

    // -- REST endpoint tests -----------------------------------------------

    #[tokio::test]
    async fn test_endpoint_create_subscription() {
        let state = make_state();
        let app = billing_router(state);
        let body = serde_json::json!({
            "tenant_id": "t1",
            "plan": {"type": "Pro"},
            "payment_method": "4242424242424242"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/billing/subscriptions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_endpoint_get_subscription_found() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Pro, None)
            .await;
        let app = billing_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/billing/subscriptions/t1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_get_subscription_not_found() {
        let state = make_state();
        let app = billing_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/billing/subscriptions/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_endpoint_upgrade_plan() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Free, None)
            .await;
        let app = billing_router(state);
        let body = serde_json::json!({"plan": {"type": "Enterprise"}});
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/billing/subscriptions/t1/plan")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_cancel_subscription() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Pro, None)
            .await;
        let app = billing_router(state);
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/billing/subscriptions/t1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_cancel_subscription_not_found() {
        let state = make_state();
        let app = billing_router(state);
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/billing/subscriptions/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_endpoint_list_invoices() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Pro, None)
            .await;
        state.manager.generate_invoice("t1", "2026-04").await;
        let app = billing_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/billing/invoices/t1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_get_invoice() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Pro, None)
            .await;
        let inv = state
            .manager
            .generate_invoice("t1", "2026-04")
            .await
            .unwrap();
        let app = billing_router(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/billing/invoices/t1/{}", inv.id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_get_invoice_not_found() {
        let state = make_state();
        let app = billing_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/billing/invoices/t1/inv_fake")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_endpoint_stripe_webhook_valid_signature() {
        let state = make_state();
        state
            .manager
            .create_subscription("t1", BillingPlan::Pro, None)
            .await;
        let body = serde_json::json!({
            "type": "customer.subscription.deleted",
            "data": {"tenant_id": "t1"}
        });
        let body_str = serde_json::to_string(&body).unwrap();
        let sig = sign_payload("test_webhook_secret", &body_str);
        let app = billing_router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/billing/webhooks/stripe")
            .header("content-type", "application/json")
            .header("Stripe-Signature", &sig)
            .body(Body::from(body_str))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_endpoint_stripe_webhook_invalid_signature() {
        let state = make_state();
        let body = serde_json::json!({
            "type": "customer.subscription.created",
            "data": {"tenant_id": "t1", "plan": "pro"}
        });
        let body_str = serde_json::to_string(&body).unwrap();
        let app = billing_router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/billing/webhooks/stripe")
            .header("content-type", "application/json")
            .header("Stripe-Signature", "invalid_sig")
            .body(Body::from(body_str))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_endpoint_stripe_webhook_no_secret_configured() {
        let state = make_state_no_secret();
        let body = serde_json::json!({
            "type": "customer.subscription.created",
            "data": {"tenant_id": "t1", "plan": "free"}
        });
        let body_str = serde_json::to_string(&body).unwrap();
        let app = billing_router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/billing/webhooks/stripe")
            .header("content-type", "application/json")
            .body(Body::from(body_str))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // No secret configured => signature check is skipped => succeeds
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -- Constant-time eq tests --------------------------------------------

    #[test]
    fn test_constant_time_eq_same() {
        assert!(constant_time_eq("abc", "abc"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq("abc", "abd"));
    }

    #[test]
    fn test_constant_time_eq_different_length() {
        assert!(!constant_time_eq("abc", "abcd"));
    }

    // -- Current period label test -----------------------------------------

    #[test]
    fn test_current_period_label_format() {
        let label = current_period_label();
        // Format: "YYYY-MM"
        assert_eq!(label.len(), 7);
        assert_eq!(&label[4..5], "-");
    }

    // -- BillingPlan serde roundtrip ----------------------------------------

    #[test]
    fn test_billing_plan_serde_roundtrip() {
        let plans = vec![
            BillingPlan::Free,
            BillingPlan::Pro,
            BillingPlan::Enterprise,
            BillingPlan::Custom {
                monthly_price_usd: 99.0,
                name: "Startup".to_string(),
            },
        ];
        for plan in plans {
            let json = serde_json::to_string(&plan).unwrap();
            let decoded: BillingPlan = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, plan);
        }
    }

    // -- Free plan invoice (no overage charge) ------------------------------

    #[tokio::test]
    async fn test_free_plan_invoice_no_overage() {
        let mgr = BillingManager::new(String::new());
        mgr.create_subscription("t1", BillingPlan::Free, None).await;
        mgr.record_usage("t1", "2026-04", 100_000, 200).await;
        let inv = mgr.generate_invoice("t1", "2026-04").await.unwrap();
        assert_eq!(inv.base_amount_usd, 0.0);
        // Free plan has 0.0 overage price, so even above limit: $0
        assert_eq!(inv.overage_amount_usd, 0.0);
        assert_eq!(inv.total_amount_usd, 0.0);
    }
}
