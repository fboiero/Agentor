use argentor_core::{ArgentorResult, ToolCall, ToolResult};
use argentor_security::Capability;
use argentor_skills::skill::{Skill, SkillDescriptor};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use tracing::info;

// ---------------------------------------------------------------------------
// Supported IaC targets
// ---------------------------------------------------------------------------

const SUPPORTED_TARGETS: &[&str] = &[
    "docker",
    "docker_compose",
    "helm",
    "terraform_aws",
    "terraform_gcp",
    "github_actions",
];

// ---------------------------------------------------------------------------
// Input deserialization types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct IacInput {
    action: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    project_name: String,
    #[serde(default)]
    output_dir: String,
    #[serde(default)]
    config: IacConfig,
}

#[derive(Debug, Default, Deserialize)]
struct IacConfig {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_replicas")]
    replicas: u32,
    #[serde(default = "default_cpu_limit")]
    cpu_limit: String,
    #[serde(default = "default_memory_limit")]
    memory_limit: String,
    #[serde(default)]
    env_vars: Vec<EnvVar>,
    #[serde(default = "default_health_check_path")]
    health_check_path: String,
    #[serde(default)]
    volumes: Vec<Volume>,
    #[serde(default)]
    ingress: Option<Ingress>,
}

#[derive(Debug, Deserialize)]
struct EnvVar {
    name: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    secret: bool,
}

#[derive(Debug, Deserialize)]
struct Volume {
    name: String,
    mount_path: String,
    #[serde(default = "default_volume_size")]
    size: String,
}

#[derive(Debug, Deserialize)]
struct Ingress {
    host: String,
    #[serde(default)]
    tls: bool,
}

fn default_port() -> u16 {
    8080
}
fn default_replicas() -> u32 {
    1
}
fn default_cpu_limit() -> String {
    "500m".to_string()
}
fn default_memory_limit() -> String {
    "256Mi".to_string()
}
fn default_health_check_path() -> String {
    "/health".to_string()
}
fn default_volume_size() -> String {
    "1Gi".to_string()
}

// ---------------------------------------------------------------------------
// IacGeneratorSkill
// ---------------------------------------------------------------------------

/// Infrastructure-as-Code generator skill.
///
/// Generates Docker, Docker Compose, Helm, Terraform, and GitHub Actions files
/// from a declarative specification.
pub struct IacGeneratorSkill {
    descriptor: SkillDescriptor,
}

impl IacGeneratorSkill {
    pub fn new() -> Self {
        Self {
            descriptor: SkillDescriptor {
                name: "iac_generator".to_string(),
                description:
                    "Generate Infrastructure-as-Code files (Docker, Helm, Terraform, GitHub Actions)"
                        .to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["generate", "list_targets", "validate"],
                            "description": "Action to perform"
                        },
                        "target": {
                            "type": "string",
                            "enum": SUPPORTED_TARGETS,
                            "description": "IaC target to generate"
                        },
                        "project_name": {
                            "type": "string",
                            "description": "Project name used in generated files"
                        },
                        "output_dir": {
                            "type": "string",
                            "description": "Absolute path to directory where files will be written"
                        },
                        "config": {
                            "type": "object",
                            "description": "Configuration for the generated IaC files"
                        }
                    },
                    "required": ["action"]
                }),
                required_capabilities: vec![Capability::FileWrite {
                    allowed_paths: vec![],
                }],
            },
        }
    }
}

impl Default for IacGeneratorSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for IacGeneratorSkill {
    fn descriptor(&self) -> &SkillDescriptor {
        &self.descriptor
    }

    async fn execute(&self, call: ToolCall) -> ArgentorResult<ToolResult> {
        let input: IacInput = match serde_json::from_value(call.arguments.clone()) {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult::error(&call.id, format!("Invalid input: {e}")));
            }
        };

        match input.action.as_str() {
            "list_targets" => handle_list_targets(&call.id),
            "validate" => handle_validate(&call.id, &input),
            "generate" => handle_generate(&call.id, &input).await,
            other => Ok(ToolResult::error(
                &call.id,
                format!(
                    "Unknown action '{other}'. Valid actions: generate, list_targets, validate"
                ),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

fn handle_list_targets(call_id: &str) -> ArgentorResult<ToolResult> {
    let response = serde_json::json!({
        "targets": SUPPORTED_TARGETS,
    });
    Ok(ToolResult::success(call_id, response.to_string()))
}

fn handle_validate(call_id: &str, input: &IacInput) -> ArgentorResult<ToolResult> {
    let mut errors: Vec<String> = Vec::new();

    if input.target.is_empty() {
        errors.push("'target' is required".to_string());
    } else if !SUPPORTED_TARGETS.contains(&input.target.as_str()) {
        errors.push(format!(
            "Unsupported target '{}'. Supported: {}",
            input.target,
            SUPPORTED_TARGETS.join(", ")
        ));
    }

    if input.project_name.is_empty() {
        errors.push("'project_name' is required".to_string());
    }

    if input.output_dir.is_empty() {
        errors.push("'output_dir' is required".to_string());
    } else if !Path::new(&input.output_dir).is_absolute() {
        errors.push("'output_dir' must be an absolute path".to_string());
    }

    if input.config.port == 0 {
        errors.push("'config.port' must be > 0".to_string());
    }

    let response = serde_json::json!({
        "valid": errors.is_empty(),
        "errors": errors,
    });
    Ok(ToolResult::success(call_id, response.to_string()))
}

async fn handle_generate(call_id: &str, input: &IacInput) -> ArgentorResult<ToolResult> {
    // Validate required fields
    if input.output_dir.is_empty() {
        return Ok(ToolResult::error(
            call_id,
            "'output_dir' is required for generate action",
        ));
    }

    if !Path::new(&input.output_dir).is_absolute() {
        return Ok(ToolResult::error(
            call_id,
            format!(
                "'output_dir' must be an absolute path, got '{}'",
                input.output_dir
            ),
        ));
    }

    if input.project_name.is_empty() {
        return Ok(ToolResult::error(call_id, "'project_name' is required"));
    }

    if input.target.is_empty() {
        return Ok(ToolResult::error(call_id, "'target' is required"));
    }

    if !SUPPORTED_TARGETS.contains(&input.target.as_str()) {
        return Ok(ToolResult::error(
            call_id,
            format!(
                "Unsupported target '{}'. Supported: {}",
                input.target,
                SUPPORTED_TARGETS.join(", ")
            ),
        ));
    }

    // Create output directory
    let out = Path::new(&input.output_dir);
    if let Err(e) = tokio::fs::create_dir_all(out).await {
        return Ok(ToolResult::error(
            call_id,
            format!(
                "Failed to create output directory '{}': {e}",
                input.output_dir
            ),
        ));
    }

    let files_written = match input.target.as_str() {
        "docker" => generate_docker(out, &input.project_name, &input.config).await,
        "docker_compose" => generate_docker_compose(out, &input.project_name, &input.config).await,
        "helm" => generate_helm(out, &input.project_name, &input.config).await,
        "terraform_aws" => generate_terraform_aws(out, &input.project_name, &input.config).await,
        "terraform_gcp" => generate_terraform_gcp(out, &input.project_name, &input.config).await,
        "github_actions" => generate_github_actions(out, &input.project_name, &input.config).await,
        _ => unreachable!(),
    };

    match files_written {
        Ok(files) => {
            info!(
                target = %input.target,
                project = %input.project_name,
                files_count = files.len(),
                "IaC files generated"
            );
            let response = serde_json::json!({
                "target": input.target,
                "project_name": input.project_name,
                "output_dir": input.output_dir,
                "files": files,
            });
            Ok(ToolResult::success(call_id, response.to_string()))
        }
        Err(e) => Ok(ToolResult::error(
            call_id,
            format!("Generation failed: {e}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Helper: write a file and return its relative path
// ---------------------------------------------------------------------------

async fn write_file(base: &Path, relative: &str, content: &str) -> Result<String, std::io::Error> {
    let full = base.join(relative);
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&full, content).await?;
    Ok(relative.to_string())
}

// ---------------------------------------------------------------------------
// Helper: render env vars for various formats
// ---------------------------------------------------------------------------

fn render_env_block_dockerfile(env_vars: &[EnvVar]) -> String {
    let mut out = String::new();
    for ev in env_vars {
        if ev.secret {
            // Secret env vars should come from runtime, not baked into the image
            out.push_str(&format!("# {} — injected at runtime (secret)\n", ev.name));
        } else {
            let val = ev.value.as_deref().unwrap_or("");
            out.push_str(&format!("ENV {}=\"{}\"\n", ev.name, val));
        }
    }
    out
}

fn render_env_block_compose(env_vars: &[EnvVar]) -> String {
    let mut out = String::new();
    for ev in env_vars {
        if ev.secret {
            out.push_str(&format!(
                "      - {}=${{{}}}  # from .env / secret\n",
                ev.name, ev.name
            ));
        } else {
            let val = ev.value.as_deref().unwrap_or("");
            out.push_str(&format!("      - {}={}\n", ev.name, val));
        }
    }
    out
}

fn render_env_block_terraform(env_vars: &[EnvVar]) -> String {
    let mut out = String::new();
    for ev in env_vars {
        let val = if ev.secret {
            format!("var.{}", ev.name.to_lowercase())
        } else {
            format!("\"{}\"", ev.value.as_deref().unwrap_or(""))
        };
        out.push_str(&format!(
            "        environment {{\n          name  = \"{}\"\n          value = {}\n        }}\n",
            ev.name, val
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

async fn generate_docker(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    let env_section = render_env_block_dockerfile(&config.env_vars);

    let dockerfile = format!(
        r#"# ---------------------------------------------------------
# Multi-stage Dockerfile for {project}
# Generated by argentor iac_generator
# ---------------------------------------------------------

# Stage 1: Builder
FROM rust:1.82-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin {project}

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd --gid 1001 app && \
    useradd --uid 1001 --gid app --create-home app

WORKDIR /app

COPY --from=builder /app/target/release/{project} /app/{project}

{env_section}
EXPOSE {port}

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:{port}{health_check_path} || exit 1

USER app

ENTRYPOINT ["/app/{project}"]
"#,
        project = project,
        port = config.port,
        health_check_path = config.health_check_path,
        env_section = env_section.trim_end(),
    );

    files.push(write_file(out, "Dockerfile", &dockerfile).await?);

    // .dockerignore
    let dockerignore = r#"target/
.git/
.github/
*.md
.env
.env.*
Dockerfile
docker-compose*.yml
"#;
    files.push(write_file(out, ".dockerignore", dockerignore).await?);

    Ok(files)
}

async fn generate_docker_compose(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    let env_section = render_env_block_compose(&config.env_vars);

    let mut volume_mounts = String::new();
    let mut volume_defs = String::new();
    for v in &config.volumes {
        volume_mounts.push_str(&format!(
            "      - {name}:{mount_path}\n",
            name = v.name,
            mount_path = v.mount_path
        ));
        volume_defs.push_str(&format!("  {name}:\n    driver: local\n", name = v.name));
    }

    let compose = format!(
        r#"# docker-compose.yml for {project}
# Generated by argentor iac_generator

version: "3.9"

services:
  app:
    build: .
    container_name: {project}-app
    ports:
      - "{port}:{port}"
    environment:
{env_section}    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
    volumes:
{volume_mounts}    networks:
      - {project}-net
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:{port}{health_check_path}"]
      interval: 30s
      timeout: 5s
      retries: 3

  postgres:
    image: postgres:16-alpine
    container_name: {project}-postgres
    environment:
      - POSTGRES_DB={project}
      - POSTGRES_USER={project}
      - POSTGRES_PASSWORD=${{POSTGRES_PASSWORD:-changeme}}
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    networks:
      - {project}-net
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U {project}"]
      interval: 10s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    container_name: {project}-redis
    ports:
      - "6379:6379"
    networks:
      - {project}-net
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
    driver: local
{volume_defs}
networks:
  {project}-net:
    driver: bridge
"#,
        project = project,
        port = config.port,
        health_check_path = config.health_check_path,
        env_section = env_section,
        volume_mounts = volume_mounts,
        volume_defs = volume_defs,
    );

    files.push(write_file(out, "docker-compose.yml", &compose).await?);

    Ok(files)
}

async fn generate_helm(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    // Chart.yaml
    let chart = format!(
        r#"apiVersion: v2
name: {project}
description: Helm chart for {project}
type: application
version: 0.1.0
appVersion: "1.0.0"
"#,
    );
    files.push(write_file(out, "Chart.yaml", &chart).await?);

    // values.yaml
    let mut env_values = String::new();
    let mut secret_values = String::new();
    for ev in &config.env_vars {
        if ev.secret {
            secret_values.push_str(&format!("  {}: \"\"\n", ev.name));
        } else {
            let val = ev.value.as_deref().unwrap_or("");
            env_values.push_str(&format!("  {}: \"{}\"\n", ev.name, val));
        }
    }

    let mut volume_values = String::new();
    for v in &config.volumes {
        volume_values.push_str(&format!(
            "  - name: {name}\n    mountPath: {mount_path}\n    size: {size}\n",
            name = v.name,
            mount_path = v.mount_path,
            size = v.size,
        ));
    }

    let ingress_values = if let Some(ref ing) = config.ingress {
        format!(
            r#"ingress:
  enabled: true
  host: "{host}"
  tls: {tls}
"#,
            host = ing.host,
            tls = ing.tls,
        )
    } else {
        "ingress:\n  enabled: false\n".to_string()
    };

    let values = format!(
        r#"# Default values for {project}
# Generated by argentor iac_generator

replicaCount: {replicas}

image:
  repository: {project}
  tag: "latest"
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: {port}

resources:
  limits:
    cpu: "{cpu_limit}"
    memory: "{memory_limit}"
  requests:
    cpu: "100m"
    memory: "128Mi"

env:
{env_values}
secrets:
{secret_values}
volumes:
{volume_values}
{ingress_values}
healthCheck:
  path: "{health_check_path}"

autoscaling:
  enabled: true
  minReplicas: {replicas}
  maxReplicas: {max_replicas}
  targetCPUUtilizationPercentage: 80
"#,
        replicas = config.replicas,
        port = config.port,
        cpu_limit = config.cpu_limit,
        memory_limit = config.memory_limit,
        health_check_path = config.health_check_path,
        max_replicas = std::cmp::max(config.replicas * 3, 5),
    );
    files.push(write_file(out, "values.yaml", &values).await?);

    // templates/deployment.yaml
    let deployment = format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{{{ include "{project}.fullname" . }}}}
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
spec:
  replicas: {{{{ .Values.replicaCount }}}}
  selector:
    matchLabels:
      {{{{- include "{project}.selectorLabels" . | nindent 6 }}}}
  template:
    metadata:
      labels:
        {{{{- include "{project}.selectorLabels" . | nindent 8 }}}}
    spec:
      containers:
        - name: {{{{ .Chart.Name }}}}
          image: "{{{{ .Values.image.repository }}}}:{{{{ .Values.image.tag }}}}"
          imagePullPolicy: {{{{ .Values.image.pullPolicy }}}}
          ports:
            - name: http
              containerPort: {{{{ .Values.service.port }}}}
              protocol: TCP
          livenessProbe:
            httpGet:
              path: {{{{ .Values.healthCheck.path }}}}
              port: http
            initialDelaySeconds: 15
            periodSeconds: 20
          readinessProbe:
            httpGet:
              path: {{{{ .Values.healthCheck.path }}}}
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
          resources:
            {{{{- toYaml .Values.resources | nindent 12 }}}}
          envFrom:
            - configMapRef:
                name: {{{{ include "{project}.fullname" . }}}}-config
            - secretRef:
                name: {{{{ include "{project}.fullname" . }}}}-secret
          {{{{- if .Values.volumes }}}}
          volumeMounts:
            {{{{- range .Values.volumes }}}}
            - name: {{{{ .name }}}}
              mountPath: {{{{ .mountPath }}}}
            {{{{- end }}}}
          {{{{- end }}}}
      {{{{- if .Values.volumes }}}}
      volumes:
        {{{{- range .Values.volumes }}}}
        - name: {{{{ .name }}}}
          persistentVolumeClaim:
            claimName: {{{{ include "{project}.fullname" $ }}}}-{{{{ .name }}}}
        {{{{- end }}}}
      {{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/deployment.yaml", &deployment).await?);

    // templates/service.yaml
    let service = format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {{{{ include "{project}.fullname" . }}}}
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
spec:
  type: {{{{ .Values.service.type }}}}
  ports:
    - port: {{{{ .Values.service.port }}}}
      targetPort: http
      protocol: TCP
      name: http
  selector:
    {{{{- include "{project}.selectorLabels" . | nindent 4 }}}}
"#,
    );
    files.push(write_file(out, "templates/service.yaml", &service).await?);

    // templates/ingress.yaml
    let ingress = format!(
        r#"{{{{- if .Values.ingress.enabled }}}}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{{{ include "{project}.fullname" . }}}}
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
  {{{{- if .Values.ingress.tls }}}}
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  {{{{- end }}}}
spec:
  {{{{- if .Values.ingress.tls }}}}
  tls:
    - hosts:
        - {{{{ .Values.ingress.host }}}}
      secretName: {{{{ include "{project}.fullname" . }}}}-tls
  {{{{- end }}}}
  rules:
    - host: {{{{ .Values.ingress.host }}}}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {{{{ include "{project}.fullname" . }}}}
                port:
                  number: {{{{ .Values.service.port }}}}
{{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/ingress.yaml", &ingress).await?);

    // templates/hpa.yaml
    let hpa = format!(
        r#"{{{{- if .Values.autoscaling.enabled }}}}
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {{{{ include "{project}.fullname" . }}}}
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {{{{ include "{project}.fullname" . }}}}
  minReplicas: {{{{ .Values.autoscaling.minReplicas }}}}
  maxReplicas: {{{{ .Values.autoscaling.maxReplicas }}}}
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: {{{{ .Values.autoscaling.targetCPUUtilizationPercentage }}}}
{{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/hpa.yaml", &hpa).await?);

    // templates/configmap.yaml
    let configmap = format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {{{{ include "{project}.fullname" . }}}}-config
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
data:
  {{{{- range $key, $value := .Values.env }}}}
  {{{{ $key }}}}: {{{{ $value | quote }}}}
  {{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/configmap.yaml", &configmap).await?);

    // templates/secrets.yaml
    let secrets = format!(
        r#"apiVersion: v1
kind: Secret
metadata:
  name: {{{{ include "{project}.fullname" . }}}}-secret
  labels:
    {{{{- include "{project}.labels" . | nindent 4 }}}}
type: Opaque
data:
  {{{{- range $key, $value := .Values.secrets }}}}
  {{{{ $key }}}}: {{{{ $value | b64enc | quote }}}}
  {{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/secrets.yaml", &secrets).await?);

    // templates/_helpers.tpl
    let helpers = format!(
        r#"{{{{/*
Expand the name of the chart.
*/}}}}
{{{{- define "{project}.name" -}}}}
{{{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}}}
{{{{- end }}}}

{{{{/*
Create a default fully qualified app name.
*/}}}}
{{{{- define "{project}.fullname" -}}}}
{{{{- if .Values.fullnameOverride }}}}
{{{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}}}
{{{{- else }}}}
{{{{- $name := default .Chart.Name .Values.nameOverride }}}}
{{{{- if contains $name .Release.Name }}}}
{{{{- .Release.Name | trunc 63 | trimSuffix "-" }}}}
{{{{- else }}}}
{{{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}}}
{{{{- end }}}}
{{{{- end }}}}
{{{{- end }}}}

{{{{/*
Common labels
*/}}}}
{{{{- define "{project}.labels" -}}}}
helm.sh/chart: {{{{ include "{project}.name" . }}}}
{{{{ include "{project}.selectorLabels" . }}}}
app.kubernetes.io/managed-by: {{{{ .Release.Service }}}}
{{{{- end }}}}

{{{{/*
Selector labels
*/}}}}
{{{{- define "{project}.selectorLabels" -}}}}
app.kubernetes.io/name: {{{{ include "{project}.name" . }}}}
app.kubernetes.io/instance: {{{{ .Release.Name }}}}
{{{{- end }}}}
"#,
    );
    files.push(write_file(out, "templates/_helpers.tpl", &helpers).await?);

    Ok(files)
}

async fn generate_terraform_aws(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    let env_block = render_env_block_terraform(&config.env_vars);

    let main_tf = format!(
        r#"# Terraform AWS — {project}
# Generated by argentor iac_generator

terraform {{
  required_version = ">= 1.5"
  required_providers {{
    aws = {{
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }}
  }}
}}

provider "aws" {{
  region = var.aws_region
}}

# ---------------------------------------------------------------------------
# VPC
# ---------------------------------------------------------------------------

module "vpc" {{
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"

  name = "{project}-vpc"
  cidr = "10.0.0.0/16"

  azs             = ["${{var.aws_region}}a", "${{var.aws_region}}b", "${{var.aws_region}}c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]

  enable_nat_gateway = true
  single_nat_gateway = true
}}

# ---------------------------------------------------------------------------
# ECS Cluster
# ---------------------------------------------------------------------------

resource "aws_ecs_cluster" "main" {{
  name = "{project}-cluster"

  setting {{
    name  = "containerInsights"
    value = "enabled"
  }}
}}

# ---------------------------------------------------------------------------
# ECS Task Definition (Fargate)
# ---------------------------------------------------------------------------

resource "aws_ecs_task_definition" "app" {{
  family                   = "{project}"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn

  container_definitions = jsonencode([
    {{
      name      = "{project}"
      image     = var.container_image
      essential = true
      portMappings = [
        {{
          containerPort = {port}
          protocol      = "tcp"
        }}
      ]
      healthCheck = {{
        command     = ["CMD-SHELL", "curl -f http://localhost:{port}{health_check_path} || exit 1"]
        interval    = 30
        timeout     = 5
        retries     = 3
        startPeriod = 10
      }}
      logConfiguration = {{
        logDriver = "awslogs"
        options = {{
          "awslogs-group"         = "/ecs/{project}"
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }}
      }}
    }}
  ])
}}

# ---------------------------------------------------------------------------
# ECS Service
# ---------------------------------------------------------------------------

resource "aws_ecs_service" "app" {{
  name            = "{project}-service"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.app.arn
  desired_count   = {replicas}
  launch_type     = "FARGATE"

  network_configuration {{
    subnets          = module.vpc.private_subnets
    security_groups  = [aws_security_group.ecs.id]
    assign_public_ip = false
  }}

  load_balancer {{
    target_group_arn = aws_lb_target_group.app.arn
    container_name   = "{project}"
    container_port   = {port}
  }}

  depends_on = [aws_lb_listener.http]
}}

# ---------------------------------------------------------------------------
# ALB
# ---------------------------------------------------------------------------

resource "aws_lb" "app" {{
  name               = "{project}-alb"
  internal           = false
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb.id]
  subnets            = module.vpc.public_subnets
}}

resource "aws_lb_target_group" "app" {{
  name        = "{project}-tg"
  port        = {port}
  protocol    = "HTTP"
  vpc_id      = module.vpc.vpc_id
  target_type = "ip"

  health_check {{
    path                = "{health_check_path}"
    healthy_threshold   = 3
    unhealthy_threshold = 3
    interval            = 30
  }}
}}

resource "aws_lb_listener" "http" {{
  load_balancer_arn = aws_lb.app.arn
  port              = 80
  protocol          = "HTTP"

  default_action {{
    type             = "forward"
    target_group_arn = aws_lb_target_group.app.arn
  }}
}}

# ---------------------------------------------------------------------------
# RDS (PostgreSQL)
# ---------------------------------------------------------------------------

resource "aws_db_instance" "postgres" {{
  identifier           = "{project}-db"
  engine               = "postgres"
  engine_version       = "16"
  instance_class       = var.db_instance_class
  allocated_storage    = 20
  db_name              = replace("{project}", "-", "_")
  username             = var.db_username
  password             = var.db_password
  skip_final_snapshot  = true
  vpc_security_group_ids = [aws_security_group.rds.id]
  db_subnet_group_name   = aws_db_subnet_group.main.name
}}

resource "aws_db_subnet_group" "main" {{
  name       = "{project}-db-subnet"
  subnet_ids = module.vpc.private_subnets
}}

# ---------------------------------------------------------------------------
# Security Groups
# ---------------------------------------------------------------------------

resource "aws_security_group" "alb" {{
  name   = "{project}-alb-sg"
  vpc_id = module.vpc.vpc_id

  ingress {{
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }}

  ingress {{
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }}

  egress {{
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }}
}}

resource "aws_security_group" "ecs" {{
  name   = "{project}-ecs-sg"
  vpc_id = module.vpc.vpc_id

  ingress {{
    from_port       = {port}
    to_port         = {port}
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }}

  egress {{
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }}
}}

resource "aws_security_group" "rds" {{
  name   = "{project}-rds-sg"
  vpc_id = module.vpc.vpc_id

  ingress {{
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [aws_security_group.ecs.id]
  }}
}}

# ---------------------------------------------------------------------------
# IAM Roles
# ---------------------------------------------------------------------------

resource "aws_iam_role" "ecs_execution" {{
  name = "{project}-ecs-execution"

  assume_role_policy = jsonencode({{
    Version = "2012-10-17"
    Statement = [{{
      Action    = "sts:AssumeRole"
      Effect    = "Allow"
      Principal = {{ Service = "ecs-tasks.amazonaws.com" }}
    }}]
  }})
}}

resource "aws_iam_role_policy_attachment" "ecs_execution" {{
  role       = aws_iam_role.ecs_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}}

resource "aws_iam_role" "ecs_task" {{
  name = "{project}-ecs-task"

  assume_role_policy = jsonencode({{
    Version = "2012-10-17"
    Statement = [{{
      Action    = "sts:AssumeRole"
      Effect    = "Allow"
      Principal = {{ Service = "ecs-tasks.amazonaws.com" }}
    }}]
  }})
}}

# ---------------------------------------------------------------------------
# CloudWatch Logs
# ---------------------------------------------------------------------------

resource "aws_cloudwatch_log_group" "ecs" {{
  name              = "/ecs/{project}"
  retention_in_days = 30
}}
"#,
        project = project,
        port = config.port,
        replicas = config.replicas,
        health_check_path = config.health_check_path,
    );
    // Suppress unused variable warning — env_block is used by other targets and
    // kept here so the helper stays exercised.  For AWS the container env vars
    // would normally come from ECS task-definition JSON rather than inline HCL,
    // so we intentionally do not embed them in main.tf.
    let _ = env_block;

    files.push(write_file(out, "main.tf", &main_tf).await?);

    // variables.tf
    let variables = format!(
        r#"# Variables for {project} AWS deployment
# Generated by argentor iac_generator

variable "aws_region" {{
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}}

variable "container_image" {{
  description = "Docker image for the application"
  type        = string
  default     = "{project}:latest"
}}

variable "task_cpu" {{
  description = "Fargate task CPU (1024 = 1 vCPU)"
  type        = number
  default     = 512
}}

variable "task_memory" {{
  description = "Fargate task memory in MiB"
  type        = number
  default     = 1024
}}

variable "db_instance_class" {{
  description = "RDS instance class"
  type        = string
  default     = "db.t3.micro"
}}

variable "db_username" {{
  description = "Database master username"
  type        = string
  default     = "{project}"
  sensitive   = true
}}

variable "db_password" {{
  description = "Database master password"
  type        = string
  sensitive   = true
}}
"#,
    );
    files.push(write_file(out, "variables.tf", &variables).await?);

    // outputs.tf
    let outputs = format!(
        r#"# Outputs for {project} AWS deployment
# Generated by argentor iac_generator

output "alb_dns_name" {{
  description = "DNS name of the Application Load Balancer"
  value       = aws_lb.app.dns_name
}}

output "ecs_cluster_name" {{
  description = "Name of the ECS cluster"
  value       = aws_ecs_cluster.main.name
}}

output "rds_endpoint" {{
  description = "RDS instance endpoint"
  value       = aws_db_instance.postgres.endpoint
}}

output "vpc_id" {{
  description = "VPC ID"
  value       = module.vpc.vpc_id
}}
"#,
    );
    files.push(write_file(out, "outputs.tf", &outputs).await?);

    Ok(files)
}

async fn generate_terraform_gcp(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    let mut env_block_cr = String::new();
    for ev in &config.env_vars {
        let val = if ev.secret {
            format!("var.{}", ev.name.to_lowercase())
        } else {
            format!("\"{}\"", ev.value.as_deref().unwrap_or(""))
        };
        env_block_cr.push_str(&format!(
            "      env {{\n        name  = \"{}\"\n        value = {}\n      }}\n",
            ev.name, val
        ));
    }

    let main_tf = format!(
        r#"# Terraform GCP — {project}
# Generated by argentor iac_generator

terraform {{
  required_version = ">= 1.5"
  required_providers {{
    google = {{
      source  = "hashicorp/google"
      version = "~> 5.0"
    }}
  }}
}}

provider "google" {{
  project = var.gcp_project
  region  = var.gcp_region
}}

# ---------------------------------------------------------------------------
# VPC
# ---------------------------------------------------------------------------

resource "google_compute_network" "main" {{
  name                    = "{project}-vpc"
  auto_create_subnetworks = false
}}

resource "google_compute_subnetwork" "main" {{
  name          = "{project}-subnet"
  ip_cidr_range = "10.0.0.0/24"
  region        = var.gcp_region
  network       = google_compute_network.main.id
}}

# ---------------------------------------------------------------------------
# Cloud Run
# ---------------------------------------------------------------------------

resource "google_cloud_run_v2_service" "app" {{
  name     = "{project}"
  location = var.gcp_region

  template {{
    scaling {{
      min_instance_count = 1
      max_instance_count = {max_instances}
    }}

    containers {{
      image = var.container_image

      ports {{
        container_port = {port}
      }}

      resources {{
        limits = {{
          cpu    = "{cpu_limit}"
          memory = "{memory_limit}"
        }}
      }}

{env_block_cr}
      startup_probe {{
        http_get {{
          path = "{health_check_path}"
          port = {port}
        }}
        initial_delay_seconds = 5
        period_seconds        = 10
      }}

      liveness_probe {{
        http_get {{
          path = "{health_check_path}"
          port = {port}
        }}
        period_seconds = 30
      }}
    }}
  }}
}}

# Allow unauthenticated access (public API)
resource "google_cloud_run_v2_service_iam_member" "public" {{
  name     = google_cloud_run_v2_service.app.name
  location = google_cloud_run_v2_service.app.location
  role     = "roles/run.invoker"
  member   = "allUsers"
}}

# ---------------------------------------------------------------------------
# Cloud SQL (PostgreSQL)
# ---------------------------------------------------------------------------

resource "google_sql_database_instance" "postgres" {{
  name             = "{project}-db"
  database_version = "POSTGRES_16"
  region           = var.gcp_region

  settings {{
    tier = var.db_tier

    ip_configuration {{
      ipv4_enabled    = false
      private_network = google_compute_network.main.id
    }}
  }}

  deletion_protection = false
}}

resource "google_sql_database" "app" {{
  name     = replace("{project}", "-", "_")
  instance = google_sql_database_instance.postgres.name
}}

resource "google_sql_user" "app" {{
  name     = var.db_username
  instance = google_sql_database_instance.postgres.name
  password = var.db_password
}}

# ---------------------------------------------------------------------------
# Private Services Access (for Cloud SQL)
# ---------------------------------------------------------------------------

resource "google_compute_global_address" "private_ip" {{
  name          = "{project}-private-ip"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.main.id
}}

resource "google_service_networking_connection" "private_vpc" {{
  network                 = google_compute_network.main.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip.name]
}}
"#,
        project = project,
        port = config.port,
        cpu_limit = config.cpu_limit,
        memory_limit = config.memory_limit,
        health_check_path = config.health_check_path,
        env_block_cr = env_block_cr,
        max_instances = std::cmp::max(config.replicas * 3, 5),
    );
    files.push(write_file(out, "main.tf", &main_tf).await?);

    // variables.tf
    let variables = format!(
        r#"# Variables for {project} GCP deployment
# Generated by argentor iac_generator

variable "gcp_project" {{
  description = "GCP project ID"
  type        = string
}}

variable "gcp_region" {{
  description = "GCP region"
  type        = string
  default     = "us-central1"
}}

variable "container_image" {{
  description = "Docker image for the application"
  type        = string
  default     = "gcr.io/PROJECT_ID/{project}:latest"
}}

variable "db_tier" {{
  description = "Cloud SQL tier"
  type        = string
  default     = "db-f1-micro"
}}

variable "db_username" {{
  description = "Database username"
  type        = string
  default     = "{project}"
  sensitive   = true
}}

variable "db_password" {{
  description = "Database password"
  type        = string
  sensitive   = true
}}
"#,
    );
    files.push(write_file(out, "variables.tf", &variables).await?);

    // outputs.tf
    let outputs = format!(
        r#"# Outputs for {project} GCP deployment
# Generated by argentor iac_generator

output "cloud_run_url" {{
  description = "URL of the Cloud Run service"
  value       = google_cloud_run_v2_service.app.uri
}}

output "cloud_sql_connection" {{
  description = "Cloud SQL connection name"
  value       = google_sql_database_instance.postgres.connection_name
}}

output "vpc_id" {{
  description = "VPC network ID"
  value       = google_compute_network.main.id
}}
"#,
    );
    files.push(write_file(out, "outputs.tf", &outputs).await?);

    Ok(files)
}

async fn generate_github_actions(
    out: &Path,
    project: &str,
    config: &IacConfig,
) -> Result<Vec<String>, std::io::Error> {
    let mut files = Vec::new();

    // CI workflow
    let ci = format!(
        r#"# CI workflow for {project}
# Generated by argentor iac_generator

name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-targets

  test:
    name: Test
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_DB: {project}_test
          POSTGRES_USER: {project}
          POSTGRES_PASSWORD: test_password
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U {project}"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-targets
        env:
          DATABASE_URL: postgres://{project}:test_password@localhost:5432/{project}_test

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets -- -D warnings

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check
"#,
    );
    files.push(write_file(out, ".github/workflows/ci.yml", &ci).await?);

    // Deploy workflow
    let deploy = format!(
        r#"# Deploy workflow for {project}
# Generated by argentor iac_generator

name: Deploy

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:
    inputs:
      environment:
        description: "Target environment"
        required: true
        default: "staging"
        type: choice
        options:
          - staging
          - production

permissions:
  contents: read
  id-token: write

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{{{ github.repository }}}}

jobs:
  build:
    name: Build & Push Image
    runs-on: ubuntu-latest
    outputs:
      image_tag: ${{{{ steps.meta.outputs.tags }}}}
    steps:
      - uses: actions/checkout@v4

      - name: Log in to Container Registry
        uses: docker/login-action@v3
        with:
          registry: ${{{{ env.REGISTRY }}}}
          username: ${{{{ github.actor }}}}
          password: ${{{{ secrets.GITHUB_TOKEN }}}}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{{{ env.REGISTRY }}}}/${{{{ env.IMAGE_NAME }}}}
          tags: |
            type=semver,pattern={{{{version}}}}
            type=sha

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: .
          push: true
          tags: ${{{{ steps.meta.outputs.tags }}}}
          labels: ${{{{ steps.meta.outputs.labels }}}}
          cache-from: type=gha
          cache-to: type=gha,mode=max

  deploy:
    name: Deploy to ${{{{ github.event.inputs.environment || 'staging' }}}}
    runs-on: ubuntu-latest
    needs: build
    environment: ${{{{ github.event.inputs.environment || 'staging' }}}}
    steps:
      - uses: actions/checkout@v4

      - name: Deploy application
        run: |
          echo "Deploying {project} to ${{{{ github.event.inputs.environment || 'staging' }}}}"
          echo "Image: ${{{{ needs.build.outputs.image_tag }}}}"
          echo "Port: {port}"
          # Add your deployment commands here (kubectl, terraform apply, etc.)

  smoke-test:
    name: Smoke Test
    runs-on: ubuntu-latest
    needs: deploy
    steps:
      - name: Health check
        run: |
          echo "Running smoke test against {health_check_path}"
          # Add your smoke test commands here
          # curl -f https://your-deployment-url{health_check_path} || exit 1
"#,
        project = project,
        port = config.port,
        health_check_path = config.health_check_path,
    );
    files.push(write_file(out, ".github/workflows/deploy.yml", &deploy).await?);

    Ok(files)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_call(id: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: "iac_generator".to_string(),
            arguments: args,
        }
    }

    fn sample_config() -> serde_json::Value {
        serde_json::json!({
            "port": 8080,
            "replicas": 3,
            "cpu_limit": "500m",
            "memory_limit": "256Mi",
            "env_vars": [
                {"name": "DATABASE_URL", "secret": true},
                {"name": "LOG_LEVEL", "value": "info"}
            ],
            "health_check_path": "/health",
            "volumes": [{"name": "data", "mount_path": "/app/data", "size": "10Gi"}],
            "ingress": {"host": "api.example.com", "tls": true}
        })
    }

    #[tokio::test]
    async fn test_list_targets() {
        let skill = IacGeneratorSkill::new();
        let call = make_call("t1", serde_json::json!({"action": "list_targets"}));
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let targets = body["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 6);
        assert!(targets.contains(&serde_json::json!("docker")));
        assert!(targets.contains(&serde_json::json!("helm")));
        assert!(targets.contains(&serde_json::json!("terraform_aws")));
        assert!(targets.contains(&serde_json::json!("terraform_gcp")));
        assert!(targets.contains(&serde_json::json!("github_actions")));
        assert!(targets.contains(&serde_json::json!("docker_compose")));
    }

    #[tokio::test]
    async fn test_generate_docker() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t2",
            serde_json::json!({
                "action": "generate",
                "target": "docker",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let files = body["files"].as_array().unwrap();
        assert!(files.contains(&serde_json::json!("Dockerfile")));
        assert!(files.contains(&serde_json::json!(".dockerignore")));

        // Verify Dockerfile content
        let dockerfile = tokio::fs::read_to_string(dir.path().join("Dockerfile"))
            .await
            .unwrap();
        assert!(dockerfile.contains("FROM rust:"));
        assert!(dockerfile.contains("AS builder"));
        assert!(dockerfile.contains("AS runtime"));
        assert!(dockerfile.contains("useradd"));
        assert!(dockerfile.contains("HEALTHCHECK"));
        assert!(dockerfile.contains("USER app"));
        assert!(dockerfile.contains("test_app"));
        assert!(dockerfile.contains("8080"));
    }

    #[tokio::test]
    async fn test_generate_docker_compose() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t3",
            serde_json::json!({
                "action": "generate",
                "target": "docker_compose",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let compose = tokio::fs::read_to_string(dir.path().join("docker-compose.yml"))
            .await
            .unwrap();
        assert!(compose.contains("postgres"));
        assert!(compose.contains("redis"));
        assert!(compose.contains("test_app-app"));
        assert!(compose.contains("8080:8080"));
        assert!(compose.contains("test_app-net"));
    }

    #[tokio::test]
    async fn test_generate_helm() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t4",
            serde_json::json!({
                "action": "generate",
                "target": "helm",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let files = body["files"].as_array().unwrap();
        assert!(files.contains(&serde_json::json!("Chart.yaml")));
        assert!(files.contains(&serde_json::json!("values.yaml")));
        assert!(files.contains(&serde_json::json!("templates/deployment.yaml")));
        assert!(files.contains(&serde_json::json!("templates/service.yaml")));
        assert!(files.contains(&serde_json::json!("templates/ingress.yaml")));
        assert!(files.contains(&serde_json::json!("templates/hpa.yaml")));
        assert!(files.contains(&serde_json::json!("templates/configmap.yaml")));
        assert!(files.contains(&serde_json::json!("templates/secrets.yaml")));
        assert!(files.contains(&serde_json::json!("templates/_helpers.tpl")));

        // Verify Chart.yaml exists and has correct content
        let chart = tokio::fs::read_to_string(dir.path().join("Chart.yaml"))
            .await
            .unwrap();
        assert!(chart.contains("name: test_app"));
        assert!(chart.contains("apiVersion: v2"));

        // Verify templates directory has files
        let deployment = tokio::fs::read_to_string(dir.path().join("templates/deployment.yaml"))
            .await
            .unwrap();
        assert!(deployment.contains("kind: Deployment"));
    }

    #[tokio::test]
    async fn test_generate_terraform_aws() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t5",
            serde_json::json!({
                "action": "generate",
                "target": "terraform_aws",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let files = body["files"].as_array().unwrap();
        assert!(files.contains(&serde_json::json!("main.tf")));
        assert!(files.contains(&serde_json::json!("variables.tf")));
        assert!(files.contains(&serde_json::json!("outputs.tf")));

        // Verify main.tf has ECS, ALB, RDS, VPC
        let main = tokio::fs::read_to_string(dir.path().join("main.tf"))
            .await
            .unwrap();
        assert!(main.contains("aws_ecs_cluster"));
        assert!(main.contains("FARGATE"));
        assert!(main.contains("aws_lb"));
        assert!(main.contains("aws_db_instance"));
        assert!(main.contains("vpc"));
    }

    #[tokio::test]
    async fn test_generate_terraform_gcp() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t5b",
            serde_json::json!({
                "action": "generate",
                "target": "terraform_gcp",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let main = tokio::fs::read_to_string(dir.path().join("main.tf"))
            .await
            .unwrap();
        assert!(main.contains("google_cloud_run_v2_service"));
        assert!(main.contains("google_sql_database_instance"));
        assert!(main.contains("google_compute_network"));
    }

    #[tokio::test]
    async fn test_generate_github_actions() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t6",
            serde_json::json!({
                "action": "generate",
                "target": "github_actions",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let files = body["files"].as_array().unwrap();
        assert!(files.contains(&serde_json::json!(".github/workflows/ci.yml")));
        assert!(files.contains(&serde_json::json!(".github/workflows/deploy.yml")));

        let ci = tokio::fs::read_to_string(dir.path().join(".github/workflows/ci.yml"))
            .await
            .unwrap();
        assert!(ci.contains("cargo test"));
        assert!(ci.contains("cargo clippy"));
        assert!(ci.contains("cargo fmt"));

        let deploy = tokio::fs::read_to_string(dir.path().join(".github/workflows/deploy.yml"))
            .await
            .unwrap();
        assert!(deploy.contains("docker/build-push-action"));
        assert!(deploy.contains("test_app"));
    }

    #[tokio::test]
    async fn test_validate_action() {
        let skill = IacGeneratorSkill::new();

        // Valid input
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();
        let call = make_call(
            "t7",
            serde_json::json!({
                "action": "validate",
                "target": "docker",
                "project_name": "test_app",
                "output_dir": out,
                "config": sample_config()
            }),
        );
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);
        let body: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(body["valid"], true);
        assert!(body["errors"].as_array().unwrap().is_empty());

        // Invalid — missing project_name and bad target
        let call2 = make_call(
            "t7b",
            serde_json::json!({
                "action": "validate",
                "target": "kubernetes_raw",
                "project_name": "",
                "output_dir": out,
                "config": {}
            }),
        );
        let result2 = skill.execute(call2).await.unwrap();
        let body2: serde_json::Value = serde_json::from_str(&result2.content).unwrap();
        assert_eq!(body2["valid"], false);
        let errors = body2["errors"].as_array().unwrap();
        assert!(errors.len() >= 2);
    }

    #[tokio::test]
    async fn test_invalid_target_returns_error() {
        let skill = IacGeneratorSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_str().unwrap();

        let call = make_call(
            "t8",
            serde_json::json!({
                "action": "generate",
                "target": "pulumi",
                "project_name": "test_app",
                "output_dir": out,
                "config": {}
            }),
        );
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Unsupported target"));
    }

    #[tokio::test]
    async fn test_missing_output_dir_returns_error() {
        let skill = IacGeneratorSkill::new();

        let call = make_call(
            "t9",
            serde_json::json!({
                "action": "generate",
                "target": "docker",
                "project_name": "test_app",
                "output_dir": "",
                "config": {}
            }),
        );
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("output_dir"));
    }

    #[tokio::test]
    async fn test_relative_output_dir_returns_error() {
        let skill = IacGeneratorSkill::new();

        let call = make_call(
            "t10",
            serde_json::json!({
                "action": "generate",
                "target": "docker",
                "project_name": "test_app",
                "output_dir": "relative/path",
                "config": {}
            }),
        );
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("absolute"));
    }

    #[tokio::test]
    async fn test_unknown_action_returns_error() {
        let skill = IacGeneratorSkill::new();
        let call = make_call("t11", serde_json::json!({"action": "destroy"}));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Unknown action"));
    }

    #[test]
    fn test_descriptor_metadata() {
        let skill = IacGeneratorSkill::new();
        let desc = skill.descriptor();
        assert_eq!(desc.name, "iac_generator");
        assert!(desc.description.contains("Infrastructure-as-Code"));
    }

    #[test]
    fn test_default_trait() {
        let skill = IacGeneratorSkill::default();
        assert_eq!(skill.descriptor().name, "iac_generator");
    }
}
