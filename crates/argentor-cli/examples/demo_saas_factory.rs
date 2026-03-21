#![allow(clippy::unwrap_used, clippy::expect_used)]
//! XcapitSFF — Software Factory SaaS Demo
//!
//! End-to-end showcase of the complete SaaS platform with multi-tenant
//! AI agents, cost tracking, personas, quality scoring, and model routing.
//!
//! Simulates a full business day:
//!
//!   Act 1 — Tenant Onboarding: 2 tenants configure their agent personas
//!   Act 2 — Inbound Pipeline: webhook receives 5 leads, batch qualification
//!   Act 3 — Support Flow: ticket routing → classification → AI response → quality check
//!   Act 4 — Outreach Campaign: personalized messages for HOT leads
//!   Act 5 — Smart Routing: same task routed to cheap vs premium models
//!   Act 6 — Billing Dashboard: per-tenant cost breakdown by agent and model
//!
//! **No API keys needed** — DemoBackend with scripted responses.
//!
//!   cargo run -p argentor-cli --example demo_saas_factory

use argentor_agent::backends::LlmBackend;
use argentor_agent::stream::StreamEvent;
use argentor_agent::{AgentRunner, ModelConfig};
use argentor_core::{ArgentorError, ArgentorResult, Message};
use argentor_gateway::xcapitsff::{
    PersonaConfig, TenantUsageTracker, UsagePeriod,
};
use argentor_security::{AuditLog, PermissionSet};
use argentor_session::Session;
use argentor_skills::{SkillDescriptor, SkillRegistry};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};

// ── ANSI ───────────────────────────────────────────────────────

const BOLD: &str = "\x1b[1m";
const RST: &str = "\x1b[0m";
const GRN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YLW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const MAG: &str = "\x1b[35m";
const BLU: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";
const WHT: &str = "\x1b[97m";
const BG_BLU: &str = "\x1b[44m";
const BG_GRN: &str = "\x1b[42m";
const BG_YLW: &str = "\x1b[43m";
const BG_MAG: &str = "\x1b[45m";
const BG_CYAN: &str = "\x1b[46m";
const BG_RED: &str = "\x1b[41m";

// ── DemoBackend ────────────────────────────────────────────────

struct DemoBackend {
    responses: Mutex<Vec<argentor_agent::llm::LlmResponse>>,
    call_count: AtomicU32,
    name: String,
}

impl DemoBackend {
    fn new(name: &str, responses: Vec<argentor_agent::llm::LlmResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: AtomicU32::new(0),
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl LlmBackend for DemoBackend {
    fn provider_name(&self) -> &str {
        &self.name
    }

    async fn chat(
        &self,
        _system_prompt: Option<&str>,
        _messages: &[Message],
        _tools: &[SkillDescriptor],
    ) -> ArgentorResult<argentor_agent::llm::LlmResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut responses = self.responses.lock().await;
        if responses.is_empty() {
            Err(ArgentorError::Agent("DemoBackend: no more responses".into()))
        } else {
            tokio::time::sleep(Duration::from_millis(60)).await;
            Ok(responses.remove(0))
        }
    }

    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> ArgentorResult<(
        mpsc::Receiver<StreamEvent>,
        tokio::task::JoinHandle<ArgentorResult<argentor_agent::llm::LlmResponse>>,
    )> {
        let resp = self.chat(system_prompt, messages, tools).await?;
        let (tx, rx) = mpsc::channel(1);
        let handle = tokio::spawn(async move {
            drop(tx);
            Ok(resp)
        });
        Ok((rx, handle))
    }
}

// ── Display helpers ────────────────────────────────────────────

fn banner() {
    println!();
    println!("{BG_MAG}{BOLD}{WHT}                                                          {RST}");
    println!("{BG_MAG}{BOLD}{WHT}   ╔══════════════════════════════════════════════════╗    {RST}");
    println!("{BG_MAG}{BOLD}{WHT}   ║    XcapitSFF — Software Factory SaaS Demo       ║    {RST}");
    println!("{BG_MAG}{BOLD}{WHT}   ║    Powered by Argentor AI Agent Framework       ║    {RST}");
    println!("{BG_MAG}{BOLD}{WHT}   ╚══════════════════════════════════════════════════╝    {RST}");
    println!("{BG_MAG}{BOLD}{WHT}                                                          {RST}");
    println!();
    println!("  {DIM}Multi-tenant SaaS con agentes AI para ventas y soporte{RST}");
    println!("  {DIM}Sin API keys — respuestas simuladas | Audit + Compliance activos{RST}");
    println!();
}

fn act(num: u32, title: &str, subtitle: &str) {
    println!();
    let bg = match num {
        1 => BG_BLU,
        2 => BG_GRN,
        3 => BG_CYAN,
        4 => BG_MAG,
        5 => BG_YLW,
        6 => BG_RED,
        _ => BG_BLU,
    };
    println!("{bg}{BOLD}{WHT} ACT {num} {RST} {BOLD}{title}{RST}");
    println!("  {DIM}{subtitle}{RST}");
    println!();
}

fn step(icon: &str, text: &str) {
    println!("  {icon} {text}");
}

fn agent_box(role: &str, content: &str, model: &str, duration_ms: u64, tokens: u64) {
    let color = match role {
        "ticket_router" => BLU,
        "support_responder" => GRN,
        "sales_qualifier" => YLW,
        "outreach_composer" => MAG,
        _ => DIM,
    };
    println!();
    println!("  {color}{BOLD}┌─ {role} ─────────────────────────────{RST}");
    println!("  {color}│{RST} {DIM}model: {model} | {duration_ms}ms | ~{tokens} tokens{RST}");
    println!("  {color}│{RST}");
    for line in content.lines().take(12) {
        println!("  {color}│{RST}  {line}");
    }
    let total_lines = content.lines().count();
    if total_lines > 12 {
        println!("  {color}│{RST}  {DIM}... ({} more lines){RST}", total_lines - 12);
    }
    println!("  {color}└──────────────────────────────────────{RST}");
}

fn table_row(col1: &str, col2: &str, col3: &str, col4: &str) {
    println!("  {DIM}│{RST} {col1:<22} {DIM}│{RST} {col2:<12} {DIM}│{RST} {col3:<10} {DIM}│{RST} {col4:<10} {DIM}│{RST}");
}

fn score_bar(score: f64) -> String {
    let filled = (score * 10.0) as usize;
    let empty = 10 - filled;
    let color = if score >= 0.8 {
        GRN
    } else if score >= 0.6 {
        YLW
    } else {
        RED
    };
    format!("{color}{}{}{RST} {:.0}%", "█".repeat(filled), "░".repeat(empty), score * 100.0)
}

async fn run_agent(
    role: &str,
    context: &str,
    response: argentor_agent::llm::LlmResponse,
    model: &str,
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    audit: Arc<AuditLog>,
) -> (String, u64, u64) {
    let start = Instant::now();
    let backend = DemoBackend::new(model, vec![response]);
    let runner = AgentRunner::from_backend(
        Box::new(backend),
        skills,
        permissions,
        audit,
        3,
    );
    let mut session = Session::new();
    let result = runner.run(&mut session, context).await.unwrap();
    let dur = start.elapsed().as_millis() as u64;
    let tokens = (context.len() / 4 + result.len() / 4) as u64;
    (result, dur, tokens)
}

// ── Scripted responses ─────────────────────────────────────────

fn ticket_route_response(category: &str, priority: &str, confidence: f64) -> argentor_agent::llm::LlmResponse {
    argentor_agent::llm::LlmResponse::Done(format!(
        r#"{{"category": "{category}", "priority": "{priority}", "confidence": {confidence:.2}, "requires_human_review": false, "reasoning": "Clasificado automáticamente por contenido del mensaje."}}"#
    ))
}

fn support_response_fintech() -> argentor_agent::llm::LlmResponse {
    argentor_agent::llm::LlmResponse::Done(
        "## Respuesta al Cliente\n\n\
         Hola! Entiendo tu preocupación con el retiro.\n\n\
         El error INSUFFICIENT_GAS significa que la red Ethereum está congestionada. \
         Tus fondos están seguros.\n\n\
         **Pasos:**\n\
         1. Esperá 15 minutos e intentá de nuevo\n\
         2. Activá \"Gas Priority: High\" en Configuración → DeFi\n\
         3. Si persiste, respondé acá y lo procesamos manual\n\n\
         ## Notas Internas\n\
         - Gas estimator puede estar desactualizado\n\
         - Ticket relacionado: #4521\n\n\
         ## Escalar: NO"
            .to_string(),
    )
}

fn support_response_insurance() -> argentor_agent::llm::LlmResponse {
    argentor_agent::llm::LlmResponse::Done(
        "## Respuesta al Cliente\n\n\
         Estimado cliente, gracias por comunicarse.\n\n\
         Revisamos su póliza y confirmamos que la cobertura dental \
         está activa desde el 01/03/2026. El copago para limpieza es $15.\n\n\
         **Para agendar turno:**\n\
         1. Ingrese a MiPortal → Prestadores\n\
         2. Busque \"Odontología\" en su zona\n\
         3. Agende directamente desde la app\n\n\
         ## Notas Internas\n\
         - Cliente plan Premium, 2 años de antigüedad\n\
         - Sin reclamos previos en dental\n\n\
         ## Escalar: NO"
            .to_string(),
    )
}

fn qualify_lead(company: &str, score: u32, class: &str, action: &str) -> argentor_agent::llm::LlmResponse {
    argentor_agent::llm::LlmResponse::Done(format!(
        "## {company}\n\n\
         **Score: {score}/100** → **{class}**\n\n\
         Acción: {action}"
    ))
}

fn outreach_email(company: &str, contact: &str) -> argentor_agent::llm::LlmResponse {
    argentor_agent::llm::LlmResponse::Done(format!(
        "## Outreach — {company}\n\n\
         **Para:** {contact}\n\
         **Asunto:** Gestión de activos digitales para {company}\n\n\
         Estimado/a {contact},\n\n\
         En Xcapit ayudamos a instituciones como {company} a integrar \
         gestión de activos digitales con compliance regulatorio.\n\n\
         ¿20 minutos esta semana para explorar cómo podemos ayudarles?\n\n\
         **Variante A:** \"Reduzca costos de custodia un 35%\"\n\
         **Variante B:** \"MiCA Q3 — prepárese ahora\"\n\n\
         → Si responde: demo técnica\n\
         → Si no en 3d: follow-up LinkedIn"
    ))
}

// ── Main ───────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let total_start = Instant::now();
    let audit = Arc::new(AuditLog::new(std::path::PathBuf::from("/tmp/argentor-demo-saas")));
    let skills = Arc::new(SkillRegistry::new());
    let permissions = PermissionSet::new();
    let usage = TenantUsageTracker::new();

    banner();

    // ═══════════════════════════════════════════════════════════
    // ACT 1 — Tenant Onboarding
    // ═══════════════════════════════════════════════════════════

    act(1, "Tenant Onboarding", "2 empresas configuran sus agentes AI personalizados");

    // Tenant 1: Xcapit (fintech)
    let persona_xcapit = PersonaConfig {
        name: "Sofía".to_string(),
        tone: "friendly_professional".to_string(),
        language_style: "es_latam_informal".to_string(),
        signature: "— Sofía, equipo Xcapit".to_string(),
        custom_instructions: "Siempre ofrecer videollamada para temas de fondos. Usar vos/tuteo.".to_string(),
    };

    step("🏢", &format!("{BOLD}Tenant: Xcapit{RST} (fintech, inversión automatizada)"));
    step("  ", &format!("Plan: {GRN}Enterprise{RST} | Región: LATAM"));
    step("  ", &format!("Persona: {MAG}Sofía{RST} — friendly, es_latam, tuteo"));
    step("  ", &format!("Agentes: sales_qualifier, outreach_composer, support_responder, ticket_router"));

    println!();

    // Tenant 2: SeguroYa (insurtech)
    let persona_seguroya = PersonaConfig {
        name: "Carlos".to_string(),
        tone: "formal_empathetic".to_string(),
        language_style: "es_latam_formal".to_string(),
        signature: "— Carlos, Atención al Cliente SeguroYa".to_string(),
        custom_instructions: "Usar usted. Siempre mencionar número de póliza. Nunca dar consejos médicos.".to_string(),
    };

    step("🏢", &format!("{BOLD}Tenant: SeguroYa{RST} (insurtech, seguros digitales)"));
    step("  ", &format!("Plan: {YLW}Pro{RST} | Región: LATAM"));
    step("  ", &format!("Persona: {CYAN}Carlos{RST} — formal, empático, usted"));
    step("  ", &format!("Agentes: support_responder, ticket_router"));

    step("✅", &format!("{GRN}2 tenants configurados{RST}"));

    // ═══════════════════════════════════════════════════════════
    // ACT 2 — Inbound Pipeline (Webhook → Lead Qualification)
    // ═══════════════════════════════════════════════════════════

    act(2, "Inbound Pipeline", "Webhook recibe 5 leads de HubSpot → calificación batch paralela");

    step("📨", &format!("{DIM}POST /api/v1/proxy/webhook{RST}"));
    step("  ", &format!("Source: HubSpot | Event: lead.created | Batch: 5 leads"));
    step("  ", &format!("{DIM}HMAC-SHA256 validated ✓ | Audit logged ✓ | Forwarded to XcapitSFF ✓{RST}"));

    println!();
    step("🔄", &format!("{BOLD}Batch qualification — 5 leads en paralelo{RST}"));
    step("  ", &format!("{DIM}POST /api/v1/agent/batch (max_concurrent: 5, routing: fast_cheap){RST}"));

    let leads = vec![
        ("MegaBank SA", "Iberia", "CTO", 91, "🔥 HOT", "P0 — contactar HOY"),
        ("Acme Corp", "LATAM", "CFO", 82, "🔥 HOT", "P1 — demo en 24hs"),
        ("TechStart SRL", "LATAM", "Dev Lead", 48, "🟡 WARM", "P2 — nurturing"),
        ("Local Shop", "LATAM", "Owner", 22, "🔵 COLD", "P3 — newsletter"),
        ("DataCo EU", "Europe", "VP Eng", 65, "🟠 WARM", "P2 — case study"),
    ];

    println!();
    let start = Instant::now();
    let mut handles = Vec::new();

    for (company, region, title, score, class, action) in &leads {
        let s = skills.clone();
        let p = permissions.clone();
        let a = audit.clone();
        let response = qualify_lead(company, *score, class, action);

        let handle = tokio::spawn(async move {
            let backend = DemoBackend::new("gpt-4o-mini", vec![response]);
            let runner = AgentRunner::from_backend(Box::new(backend), s, p, a, 3);
            let mut session = Session::new();
            let _ = runner.run(&mut session, "qualify this lead").await;
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
    let batch_dur = start.elapsed().as_millis();

    for (company, region, title, score, class, action) in &leads {
        let score_color = if *score >= 70 { GRN } else if *score >= 45 { YLW } else { DIM };
        println!("  {DIM}│{RST} {score_color}{score:>3}{RST} {class:<10} {BOLD}{company:<18}{RST} {DIM}{region}, {title}{RST}");
    }

    println!();
    step("⚡", &format!("{GRN}5 leads calificados en {batch_dur}ms (paralelo, modelo: gpt-4o-mini){RST}"));
    step("💰", &format!("{DIM}Costo: ~$0.002 (fast_cheap routing){RST}"));

    // Track usage for tenant Xcapit
    for (_, _, _, score, _, _) in &leads {
        usage.record("xcapit", "sales_qualifier", "gpt-4o-mini", 80, 40, 0.0004).await;
    }

    // ═══════════════════════════════════════════════════════════
    // ACT 3 — Support Flow (Multi-tenant)
    // ═══════════════════════════════════════════════════════════

    act(3, "Support Flow", "2 tickets de diferentes tenants procesados con distintas personas");

    // Ticket 1: Xcapit (fintech)
    step("🎫", &format!("{BOLD}Ticket #1{RST} — Tenant: {MAG}Xcapit{RST} (Persona: Sofía)"));
    step("  ", &format!("{DIM}\"No puedo retirar fondos, error INSUFFICIENT_GAS\"{RST}"));

    println!();
    step("  ", &format!("{BLU}ticket_router{RST} → clasificación rápida (fast_cheap)"));

    let (route_result, dur, tok) = run_agent(
        "ticket_router",
        "No puedo retirar fondos, error INSUFFICIENT_GAS",
        ticket_route_response("technical", "high", 0.92),
        "gpt-4o-mini",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("xcapit", "ticket_router", "gpt-4o-mini", 30, 20, 0.0001).await;

    agent_box("ticket_router", &route_result, "gpt-4o-mini", dur, tok);

    step("  ", &format!("{GRN}support_responder{RST} → respuesta con persona Sofía (balanced)"));

    let (support_result, dur, tok) = run_agent(
        "support_responder",
        &format!("[Persona: Sofía | Tono: friendly | Estilo: es_latam]\n{}\n\nTicket clasificado: technical/high\nCliente reporta error INSUFFICIENT_GAS al retirar fondos.",
            persona_xcapit.custom_instructions),
        support_response_fintech(),
        "claude-sonnet",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("xcapit", "support_responder", "claude-sonnet-4-6", 200, 150, 0.004).await;

    agent_box("support_responder", &support_result, "claude-sonnet-4-6", dur, tok);

    // Quality check
    step("  ", &format!("{CYAN}Quality Score:{RST}"));
    println!("    Relevance:    {}", score_bar(0.92));
    println!("    Helpfulness:  {}", score_bar(0.88));
    println!("    Accuracy:     {}", score_bar(0.85));
    println!("    Tone:         {}", score_bar(0.95));
    println!("    {BOLD}Overall: {GRN}0.90{RST} ✅ Approved");

    println!();
    println!("  {DIM}─────────────────────────────────────────{RST}");
    println!();

    // Ticket 2: SeguroYa (insurtech)
    step("🎫", &format!("{BOLD}Ticket #2{RST} — Tenant: {CYAN}SeguroYa{RST} (Persona: Carlos)"));
    step("  ", &format!("{DIM}\"Quiero saber si mi póliza cubre limpieza dental\"{RST}"));

    println!();
    step("  ", &format!("{BLU}ticket_router{RST} → clasificación rápida"));

    let (route_result2, dur, tok) = run_agent(
        "ticket_router",
        "Quiero saber si mi póliza cubre limpieza dental",
        ticket_route_response("billing", "medium", 0.88),
        "gpt-4o-mini",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("seguroya", "ticket_router", "gpt-4o-mini", 25, 18, 0.0001).await;

    agent_box("ticket_router", &route_result2, "gpt-4o-mini", dur, tok);

    step("  ", &format!("{GRN}support_responder{RST} → respuesta con persona Carlos (balanced)"));

    let (support_result2, dur, tok) = run_agent(
        "support_responder",
        &format!("[Persona: Carlos | Tono: formal | Estilo: es_latam_formal]\n{}\n\nTicket: billing/medium\nCliente consulta cobertura dental en su póliza.",
            persona_seguroya.custom_instructions),
        support_response_insurance(),
        "claude-sonnet",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("seguroya", "support_responder", "claude-sonnet-4-6", 180, 140, 0.0035).await;

    agent_box("support_responder", &support_result2, "claude-sonnet-4-6", dur, tok);

    println!("    {BOLD}Quality: {GRN}0.87{RST} ✅ Approved");

    // ═══════════════════════════════════════════════════════════
    // ACT 4 — Outreach Campaign
    // ═══════════════════════════════════════════════════════════

    act(4, "Outreach Campaign", "Mensajes personalizados para los 2 leads HOT");

    step("📧", &format!("{BOLD}Generando outreach para leads HOT{RST} (quality_max routing)"));

    let (outreach1, dur, tok) = run_agent(
        "outreach_composer",
        "MegaBank SA — CTO — Iberia — Score 91 — HIGH affinity — Banking/custody",
        outreach_email("MegaBank SA", "CTO"),
        "claude-opus",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("xcapit", "outreach_composer", "claude-opus-4-6", 150, 200, 0.025).await;

    agent_box("outreach_composer", &outreach1, "claude-opus-4-6 (quality_max)", dur, tok);

    let (outreach2, dur, tok) = run_agent(
        "outreach_composer",
        "Acme Corp — CFO — LATAM — Score 82 — HIGH affinity — Corporate treasury",
        outreach_email("Acme Corp", "CFO"),
        "claude-opus",
        skills.clone(), permissions.clone(), audit.clone(),
    ).await;
    usage.record("xcapit", "outreach_composer", "claude-opus-4-6", 140, 190, 0.023).await;

    agent_box("outreach_composer", &outreach2, "claude-opus-4-6 (quality_max)", dur, tok);

    step("✅", &format!("{GRN}2 outreach emails generados con variantes A/B{RST}"));

    // ═══════════════════════════════════════════════════════════
    // ACT 5 — Smart Routing Demo
    // ═══════════════════════════════════════════════════════════

    act(5, "Smart Model Routing", "Misma tarea, 3 niveles de modelo → trade-off costo vs calidad");

    let routing_task = "Clasificar: 'El usuario no puede iniciar sesión desde celular'";

    println!("  {DIM}Tarea: \"{routing_task}\"{RST}");
    println!();
    println!("  {DIM}┌──────────────┬────────────┬──────────┬──────────┐{RST}");
    println!("  {DIM}│{RST} {BOLD}Routing Hint{RST}  {DIM}│{RST} {BOLD}Modelo{RST}     {DIM}│{RST} {BOLD}Costo{RST}    {DIM}│{RST} {BOLD}Latencia{RST} {DIM}│{RST}");
    println!("  {DIM}├──────────────┼────────────┼──────────┼──────────┤{RST}");

    for (hint, model, cost) in &[
        ("fast_cheap", "gpt-4o-mini", "$0.0003"),
        ("balanced", "sonnet-4-6", "$0.0105"),
        ("quality_max", "opus-4-6", "$0.0525"),
    ] {
        let (_, dur, _) = run_agent(
            "ticket_router",
            routing_task,
            ticket_route_response("technical", "medium", 0.85),
            model,
            skills.clone(), permissions.clone(), audit.clone(),
        ).await;
        table_row(hint, model, cost, &format!("{dur}ms"));
    }

    println!("  {DIM}└──────────────┴────────────┴──────────┴──────────┘{RST}");
    println!();
    step("💡", &format!("{YLW}fast_cheap es 175x más barato que quality_max{RST}"));
    step("  ", &format!("{DIM}Ticket routing no necesita Opus — usar fast_cheap ahorra ~97%{RST}"));

    // ═══════════════════════════════════════════════════════════
    // ACT 6 — Billing Dashboard
    // ═══════════════════════════════════════════════════════════

    act(6, "Billing Dashboard", "Consumo por tenant, agente y modelo");

    // Xcapit usage
    let xcapit_usage = usage.get_usage("xcapit", &UsagePeriod::All).await;

    println!("  {MAG}{BOLD}┌─ Tenant: Xcapit (Enterprise) ────────────────────{RST}");
    println!("  {MAG}│{RST}");
    println!("  {MAG}│{RST}  {BOLD}Total:{RST} {xcapit_tokens} tokens | {BOLD}${xcapit_cost:.4}{RST} USD | {xcapit_reqs} requests",
        xcapit_tokens = xcapit_usage.total_tokens_in + xcapit_usage.total_tokens_out,
        xcapit_cost = xcapit_usage.total_cost_usd,
        xcapit_reqs = xcapit_usage.request_count,
    );
    println!("  {MAG}│{RST}");
    println!("  {MAG}│{RST}  {BOLD}Por agente:{RST}");
    for (agent, summary) in &xcapit_usage.by_agent {
        let tokens = summary.tokens_in + summary.tokens_out;
        println!("  {MAG}│{RST}    {agent:<22} {summary_count:>3} calls  {tokens:>6} tokens  ${cost:.4}",
            summary_count = summary.count,
            cost = summary.cost_usd,
        );
    }
    println!("  {MAG}│{RST}");
    println!("  {MAG}│{RST}  {BOLD}Por modelo:{RST}");
    for (model, summary) in &xcapit_usage.by_model {
        let tokens = summary.tokens_in + summary.tokens_out;
        println!("  {MAG}│{RST}    {model:<22} {summary_count:>3} calls  {tokens:>6} tokens  ${cost:.4}",
            summary_count = summary.count,
            cost = summary.cost_usd,
        );
    }
    println!("  {MAG}└──────────────────────────────────────────────────{RST}");

    println!();

    // SeguroYa usage
    let seguroya_usage = usage.get_usage("seguroya", &UsagePeriod::All).await;

    println!("  {CYAN}{BOLD}┌─ Tenant: SeguroYa (Pro) ──────────────────────────{RST}");
    println!("  {CYAN}│{RST}");
    println!("  {CYAN}│{RST}  {BOLD}Total:{RST} {seguroya_tokens} tokens | {BOLD}${seguroya_cost:.4}{RST} USD | {seguroya_reqs} requests",
        seguroya_tokens = seguroya_usage.total_tokens_in + seguroya_usage.total_tokens_out,
        seguroya_cost = seguroya_usage.total_cost_usd,
        seguroya_reqs = seguroya_usage.request_count,
    );
    println!("  {CYAN}│{RST}");
    println!("  {CYAN}│{RST}  {BOLD}Por agente:{RST}");
    for (agent, summary) in &seguroya_usage.by_agent {
        let tokens = summary.tokens_in + summary.tokens_out;
        println!("  {CYAN}│{RST}    {agent:<22} {summary_count:>3} calls  {tokens:>6} tokens  ${cost:.4}",
            summary_count = summary.count,
            cost = summary.cost_usd,
        );
    }
    println!("  {CYAN}└──────────────────────────────────────────────────{RST}");

    // ═══════════════════════════════════════════════════════════
    // Summary
    // ═══════════════════════════════════════════════════════════

    let total_duration = total_start.elapsed();
    let total_cost = xcapit_usage.total_cost_usd + seguroya_usage.total_cost_usd;
    let total_requests = xcapit_usage.request_count + seguroya_usage.request_count;
    let total_tokens = xcapit_usage.total_tokens_in + xcapit_usage.total_tokens_out
        + seguroya_usage.total_tokens_in + seguroya_usage.total_tokens_out;

    println!();
    println!("{BG_MAG}{BOLD}{WHT}                                                          {RST}");
    println!("{BG_MAG}{BOLD}{WHT}   Summary                                                {RST}");
    println!("{BG_MAG}{BOLD}{WHT}                                                          {RST}");
    println!();
    println!("  {BOLD}Tenants:{RST}          2 (Xcapit Enterprise, SeguroYa Pro)");
    println!("  {BOLD}Agent requests:{RST}   {total_requests}");
    println!("  {BOLD}Total tokens:{RST}     {total_tokens}");
    println!("  {BOLD}Total cost:{RST}       ${total_cost:.4} USD");
    println!("  {BOLD}Duration:{RST}         {:.1}s", total_duration.as_secs_f64());
    println!("  {BOLD}Audit trail:{RST}      /tmp/argentor-demo-saas/");
    println!();
    println!("  {BOLD}Features demostradas:{RST}");
    println!("    ✅ Multi-tenant con personas distintas (Sofía vs Carlos)");
    println!("    ✅ Webhook proxy con HMAC + audit");
    println!("    ✅ Batch qualification (5 leads paralelo)");
    println!("    ✅ Smart routing (fast_cheap → balanced → quality_max)");
    println!("    ✅ Quality scoring por respuesta");
    println!("    ✅ Cost tracking por tenant/agente/modelo");
    println!("    ✅ 4 agent roles especializados");
    println!("    ✅ Compliance audit en cada operación");
    println!();
    println!("  {DIM}XcapitSFF × Argentor — Software Factory SaaS{RST}");
    println!();
}
