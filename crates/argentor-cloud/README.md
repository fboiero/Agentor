# argentor-cloud

**Managed cloud runtime for Argentor — multi-tenant agent hosting, billing, dashboard.**

## What this is

`argentor-cloud` is the scaffolding for **Argentor Cloud**, a hosted SaaS offering for the Argentor agent framework. It is to Argentor what LangSmith is to LangChain: a managed runtime, observability, and billing layer that customers can subscribe to instead of operating their own deployments.

## Why it matters

Today, Argentor is a framework: users clone, build, and run it themselves. LangChain, LlamaIndex, and Anthropic all ship commercial managed offerings alongside their OSS — that's where revenue and enterprise adoption live. Without a managed story, Argentor stays a framework on GitHub. This crate is step one of closing that gap.

## Status

**v1.x — in-memory scaffolding.** Every manager in this crate is a stub that keeps state in `RwLock<HashMap<...>>`. The types, traits, and module layout are designed so the v2.x production impl can swap backends without touching the public API.

Production deployment requires:

- **PostgreSQL** — tenants, quota ledger, invoices
- **Redis** — active session cache, quota increments, scheduler queue
- **Object storage (S3)** — audit logs, session artifacts, exports
- **Stripe / Paddle** — billing provider (behind feature flag)
- **CDN + SPA** — hosted dashboard frontend
- **Kubernetes** — tenant-isolated agent workers

## Architecture overview

```text
                         ┌────────────────┐
 dashboard (SPA)  ◄──────┤ DashboardAdapter│
                         └────────┬───────┘
                                  │ snapshot()
 ┌──────────────┐  run()  ┌───────▼────────┐  check() / record()  ┌───────────────┐
 │ API gateway  ├────────►│ ManagedRuntime │◄────────────────────►│ QuotaEnforcer │
 └──────┬───────┘         └───────┬────────┘                      └───────┬───────┘
        │                         │                                       │
        │                         │ audit                                 │
        │                 ┌───────▼────────┐                              │
        │                 │   AuditLog     │                              │
        │                 └────────────────┘                              │
        │                                                                 │
        │    ┌──────────────────┐              ┌────────────────┐         │
        └───►│ TenantManager    │              │ CloudScheduler │         │
             └──────────────────┘              └────────────────┘         │
                                                                          │
                                   ┌──────────────────┐                   │
                                   │ UsageMeter       ├──────────────────►│
                                   │  + BillingProv.  │   end-of-period   │
                                   └──────────────────┘                   │
```

## Modules

| Module      | Purpose                                              |
|-------------|------------------------------------------------------|
| `tenant`    | Tenant records, plans, data-region, lifecycle        |
| `quota`     | Usage ledger + per-plan limit enforcement            |
| `runtime`   | Tenant-isolated wrapper around `AgentRunner`         |
| `dashboard` | Snapshot generator for the hosted frontend           |
| `billing`   | Invoice generation + pluggable payment provider      |
| `scheduler` | Priority queue for scheduled agent runs              |
| `audit`     | Append-only audit log with pluggable sinks           |

## Pricing model (indicative)

| Plan        | Monthly runs | Active agents | Tokens/mo | Price      |
|-------------|--------------|---------------|-----------|------------|
| Free        | 1,000        | 5             | 1M        | $0         |
| Starter     | 50,000       | 25            | 50M       | $99/mo     |
| Growth      | 500,000      | 100           | 500M      | $499/mo    |
| Enterprise  | Unlimited    | Unlimited     | Unlimited | Contact us |

Pricing is encoded in `TenantPlan::{run_quota, token_quota, agent_quota, storage_mb_quota}`. Change there propagates everywhere.

## Data residency

`DataRegion` supports `UsEast`, `UsWest`, `EuWest`, `EuCentral`, `ApSouth`, `ApSoutheast`. EU regions are flagged via `DataRegion::is_gdpr()` so downstream modules can enforce locality.

## Roadmap to MVP

1. **v1.0 (current)** — scaffolding, types, in-memory stubs, tests
2. **v1.1** — PostgreSQL tenant + quota persistence (sqlx)
3. **v1.2** — Stripe integration behind `stripe` feature flag
4. **v1.3** — Audit sink: S3 Parquet archiver
5. **v1.4** — Real runtime wired to `argentor-agent::AgentRunner`
6. **v1.5** — SSE/WebSocket push from `DashboardAdapter` via `argentor-gateway`
7. **v2.0** — Kubernetes operator for tenant-isolated workers; public beta

## License

AGPL-3.0-only, same as the rest of Argentor.
