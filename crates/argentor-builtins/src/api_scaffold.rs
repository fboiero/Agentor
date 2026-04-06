use argentor_core::{ArgentorResult, ToolCall, ToolResult};
use argentor_security::{Capability, PermissionSet};
use argentor_skills::skill::{Skill, SkillDescriptor};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

/// Supported API frameworks for scaffold generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    RustAxum,
    PythonFastapi,
    NodeExpress,
}

impl Framework {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::RustAxum => "Rust + Axum",
            Self::PythonFastapi => "Python + FastAPI",
            Self::NodeExpress => "Node.js + Express",
        }
    }

    /// Return the string key expected in JSON input.
    pub fn key(&self) -> &'static str {
        match self {
            Self::RustAxum => "rust_axum",
            Self::PythonFastapi => "python_fastapi",
            Self::NodeExpress => "node_express",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "rust_axum" => Some(Self::RustAxum),
            "python_fastapi" => Some(Self::PythonFastapi),
            "node_express" => Some(Self::NodeExpress),
            _ => None,
        }
    }
}

const ALL_FRAMEWORKS: [Framework; 3] = [
    Framework::RustAxum,
    Framework::PythonFastapi,
    Framework::NodeExpress,
];

/// An API endpoint definition coming from the specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSpec {
    pub method: String,
    pub path: String,
    pub handler: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub request_body: Option<serde_json::Value>,
    #[serde(default)]
    pub response: Option<serde_json::Value>,
}

/// A model field definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

/// A data model definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    pub fields: Vec<FieldSpec>,
}

/// Full scaffold specification parsed from the JSON input.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScaffoldSpec {
    project_name: String,
    #[serde(default)]
    description: String,
    output_dir: String,
    framework: String,
    #[serde(default)]
    endpoints: Vec<EndpointSpec>,
    #[serde(default)]
    models: Vec<ModelSpec>,
    #[serde(default = "default_database")]
    database: String,
}

fn default_database() -> String {
    "sqlite".to_string()
}

// ---------------------------------------------------------------------------
// File entry produced by generators
// ---------------------------------------------------------------------------

/// A file to be written to disk.
struct GeneratedFile {
    /// Relative path inside the project directory.
    relative_path: String,
    /// File content.
    content: String,
}

// ---------------------------------------------------------------------------
// Skill implementation
// ---------------------------------------------------------------------------

/// Builtin skill that generates complete API project scaffolds from a JSON
/// specification.  Supports Rust/Axum, Python/FastAPI, and Node/Express.
pub struct ApiScaffoldSkill {
    descriptor: SkillDescriptor,
}

impl ApiScaffoldSkill {
    pub fn new() -> Self {
        Self {
            descriptor: SkillDescriptor {
                name: "api_scaffold".to_string(),
                description: "Generate complete API project scaffolds from specifications"
                    .to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["generate", "list_frameworks"],
                            "description": "Action to perform"
                        },
                        "framework": {
                            "type": "string",
                            "enum": ["rust_axum", "python_fastapi", "node_express"],
                            "description": "Target framework (required for generate)"
                        },
                        "project_name": {
                            "type": "string",
                            "description": "Name of the project (required for generate)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Short project description"
                        },
                        "output_dir": {
                            "type": "string",
                            "description": "Absolute path for the output directory (required for generate)"
                        },
                        "endpoints": {
                            "type": "array",
                            "description": "API endpoint definitions",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "method": { "type": "string" },
                                    "path": { "type": "string" },
                                    "handler": { "type": "string" },
                                    "description": { "type": "string" },
                                    "request_body": { "type": "object" },
                                    "response": { "type": "object" }
                                },
                                "required": ["method", "path", "handler"]
                            }
                        },
                        "models": {
                            "type": "array",
                            "description": "Database model definitions",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "fields": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "name": { "type": "string" },
                                                "type": { "type": "string" }
                                            },
                                            "required": ["name", "type"]
                                        }
                                    }
                                },
                                "required": ["name", "fields"]
                            }
                        },
                        "database": {
                            "type": "string",
                            "enum": ["sqlite", "postgresql"],
                            "description": "Database backend (default: sqlite)"
                        }
                    },
                    "required": ["action"]
                }),
                required_capabilities: vec![Capability::FileWrite {
                    allowed_paths: vec![], // Configured at runtime
                }],
            },
        }
    }
}

impl Default for ApiScaffoldSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for ApiScaffoldSkill {
    fn descriptor(&self) -> &SkillDescriptor {
        &self.descriptor
    }

    fn validate_arguments(
        &self,
        call: &ToolCall,
        permissions: &PermissionSet,
    ) -> ArgentorResult<()> {
        let action = call.arguments["action"].as_str().unwrap_or_default();
        if action != "generate" {
            return Ok(());
        }

        let output_dir = call.arguments["output_dir"].as_str().unwrap_or_default();
        if output_dir.is_empty() {
            return Ok(()); // Will be caught in execute()
        }

        let path = Path::new(output_dir);
        let canonical = if path.exists() {
            match path.canonicalize() {
                Ok(p) => p,
                Err(_) => return Ok(()),
            }
        } else if let Some(parent) = path.parent() {
            if parent.exists() {
                match parent.canonicalize() {
                    Ok(p) => p.join(path.file_name().unwrap_or_default()),
                    Err(_) => return Ok(()),
                }
            } else {
                path.to_path_buf()
            }
        } else {
            return Ok(());
        };

        if !permissions.check_file_write_path(&canonical) {
            return Err(argentor_core::ArgentorError::Security(format!(
                "file write not permitted for path '{}'",
                canonical.display()
            )));
        }

        Ok(())
    }

    async fn execute(&self, call: ToolCall) -> ArgentorResult<ToolResult> {
        let action = call.arguments["action"].as_str().unwrap_or_default();

        match action {
            "list_frameworks" => {
                let frameworks: Vec<serde_json::Value> = ALL_FRAMEWORKS
                    .iter()
                    .map(|f| {
                        serde_json::json!({
                            "key": f.key(),
                            "label": f.label(),
                        })
                    })
                    .collect();
                let response = serde_json::json!({ "frameworks": frameworks });
                Ok(ToolResult::success(&call.id, response.to_string()))
            }
            "generate" => self.handle_generate(&call).await,
            "" => Ok(ToolResult::error(
                &call.id,
                "Missing required field: 'action'",
            )),
            other => Ok(ToolResult::error(
                &call.id,
                format!("Unknown action: '{other}'. Use 'generate' or 'list_frameworks'."),
            )),
        }
    }
}

impl ApiScaffoldSkill {
    async fn handle_generate(&self, call: &ToolCall) -> ArgentorResult<ToolResult> {
        // --- Validate required fields ----------------------------------------
        let framework_str = match call.arguments["framework"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => {
                return Ok(ToolResult::error(
                    &call.id,
                    "Missing required field: 'framework'",
                ))
            }
        };

        let framework = match Framework::from_str(framework_str) {
            Some(f) => f,
            None => {
                let valid: Vec<&str> = ALL_FRAMEWORKS.iter().map(Framework::key).collect();
                return Ok(ToolResult::error(
                    &call.id,
                    format!(
                        "Invalid framework: '{framework_str}'. Valid options: {}",
                        valid.join(", ")
                    ),
                ));
            }
        };

        let project_name = match call.arguments["project_name"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return Ok(ToolResult::error(
                    &call.id,
                    "Missing required field: 'project_name'",
                ))
            }
        };

        let output_dir = match call.arguments["output_dir"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return Ok(ToolResult::error(
                    &call.id,
                    "Missing required field: 'output_dir'",
                ))
            }
        };

        let output_path = Path::new(&output_dir);
        if !output_path.is_absolute() {
            return Ok(ToolResult::error(
                &call.id,
                format!("output_dir must be absolute: '{output_dir}'"),
            ));
        }

        // Parse the spec from the full arguments value.
        let spec = ScaffoldSpec {
            project_name: project_name.clone(),
            description: call.arguments["description"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            output_dir: output_dir.clone(),
            framework: framework_str.to_string(),
            endpoints: serde_json::from_value(call.arguments["endpoints"].clone())
                .unwrap_or_default(),
            models: serde_json::from_value(call.arguments["models"].clone()).unwrap_or_default(),
            database: call.arguments["database"]
                .as_str()
                .unwrap_or("sqlite")
                .to_string(),
        };

        // --- Generate files --------------------------------------------------
        let files = match framework {
            Framework::RustAxum => generate_rust_axum(&spec),
            Framework::PythonFastapi => generate_python_fastapi(&spec),
            Framework::NodeExpress => generate_node_express(&spec),
        };

        // --- Write to disk ---------------------------------------------------
        let base = Path::new(&output_dir);
        if let Err(e) = tokio::fs::create_dir_all(base).await {
            return Ok(ToolResult::error(
                &call.id,
                format!("Failed to create output directory '{output_dir}': {e}"),
            ));
        }

        let mut written_paths: Vec<String> = Vec::new();

        for file in &files {
            let full_path = base.join(&file.relative_path);
            if let Some(parent) = full_path.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return Ok(ToolResult::error(
                        &call.id,
                        format!("Failed to create directory '{}': {e}", parent.display()),
                    ));
                }
            }
            if let Err(e) = tokio::fs::write(&full_path, &file.content).await {
                return Ok(ToolResult::error(
                    &call.id,
                    format!("Failed to write '{}': {e}", full_path.display()),
                ));
            }
            written_paths.push(file.relative_path.clone());
        }

        info!(
            project = %project_name,
            framework = %framework_str,
            files = written_paths.len(),
            output = %output_dir,
            "API scaffold generated"
        );

        let response = serde_json::json!({
            "project_name": project_name,
            "framework": framework_str,
            "output_dir": output_dir,
            "files_created": written_paths,
            "file_count": written_paths.len(),
        });

        Ok(ToolResult::success(&call.id, response.to_string()))
    }
}

// ===========================================================================
// Rust / Axum generator
// ===========================================================================

fn generate_rust_axum(spec: &ScaffoldSpec) -> Vec<GeneratedFile> {
    let name = &spec.project_name;
    let desc = if spec.description.is_empty() {
        format!("{name} API")
    } else {
        spec.description.clone()
    };

    let db_dep = match spec.database.as_str() {
        "postgresql" => {
            r#"sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres"] }"#
        }
        _ => r#"sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite"] }"#,
    };

    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
description = "{desc}"

[dependencies]
axum = "0.7"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
{db_dep}
tower-http = {{ version = "0.5", features = ["cors", "trace"] }}
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter"] }}
uuid = {{ version = "1", features = ["v4"] }}
chrono = {{ version = "0.4", features = ["serde"] }}
"#
    );

    // Build route registrations and handler functions
    let mut route_lines = String::new();
    let mut handler_fns = String::new();

    for ep in &spec.endpoints {
        let method_lower = ep.method.to_lowercase();
        let axum_method = match method_lower.as_str() {
            "get" => "get",
            "post" => "post",
            "put" => "put",
            "delete" => "delete",
            "patch" => "patch",
            _ => "get",
        };
        // Convert :param to {param} for axum
        let axum_path = ep
            .path
            .split('/')
            .map(|seg| {
                if let Some(stripped) = seg.strip_prefix(':') {
                    format!("{{{stripped}}}")
                } else {
                    seg.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("/");

        route_lines.push_str(&format!(
            "        .route(\"{axum_path}\", {axum_method}({handler}))\n",
            handler = ep.handler
        ));

        // Generate handler function
        let has_path_params = ep.path.contains(':');
        let has_body = ep.request_body.is_some();

        let params = if has_path_params && has_body {
            "Path(params): Path<std::collections::HashMap<String, String>>, Json(body): Json<serde_json::Value>"
        } else if has_path_params {
            "Path(params): Path<std::collections::HashMap<String, String>>"
        } else if has_body {
            "Json(body): Json<serde_json::Value>"
        } else {
            ""
        };

        let comment = if ep.description.is_empty() {
            format!("Handler for {} {}", ep.method, ep.path)
        } else {
            ep.description.clone()
        };

        handler_fns.push_str(&format!(
            r#"/// {comment}
async fn {handler}({params}) -> impl IntoResponse {{
    // Add your business logic here
    Json(serde_json::json!({{ "handler": "{handler}", "message": "endpoint ready" }}))
}}

"#,
            handler = ep.handler,
        ));
    }

    // Model structs
    let mut model_code = String::new();
    for model in &spec.models {
        model_code.push_str(&format!(
            "#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]\npub struct {} {{\n",
            model.name
        ));
        for field in &model.fields {
            let rust_type = map_type_to_rust(&field.field_type);
            model_code.push_str(&format!("    pub {}: {},\n", field.name, rust_type));
        }
        model_code.push_str("}\n\n");
    }

    let db_url_env = match spec.database.as_str() {
        "postgresql" => "DATABASE_URL",
        _ => "DATABASE_URL",
    };

    let main_rs = format!(
        r#"use axum::{{extract::Path, response::IntoResponse, routing::{{get, post, put, delete, patch}}, Json, Router}};
use serde::{{Deserialize, Serialize}};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

{model_code}
// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

{handler_fns}
// ---------------------------------------------------------------------------
// Application setup
// ---------------------------------------------------------------------------

fn app() -> Router {{
    Router::new()
{route_lines}        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}}

#[tokio::main]
async fn main() {{
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let _db_url = std::env::var("{db_url_env}").unwrap_or_else(|_| "sqlite:data.db".to_string());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Listening on {{addr}}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind failed");
    axum::serve(listener, app()).await.expect("server error");
}}
"#
    );

    let dockerfile = format!(
        r#"FROM rust:1.77-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/{name} /usr/local/bin/app
EXPOSE 3000
CMD ["app"]
"#
    );

    let readme = format!(
        r#"# {name}

{desc}

## Getting started

```bash
cargo run
```

The server will start on `http://localhost:3000`.

## Endpoints

{}

## Database

Backend: {}
"#,
        spec.endpoints
            .iter()
            .map(|e| format!("- `{} {}` - {}", e.method, e.path, e.description))
            .collect::<Vec<_>>()
            .join("\n"),
        spec.database,
    );

    vec![
        GeneratedFile {
            relative_path: "Cargo.toml".to_string(),
            content: cargo_toml,
        },
        GeneratedFile {
            relative_path: "src/main.rs".to_string(),
            content: main_rs,
        },
        GeneratedFile {
            relative_path: "src/models.rs".to_string(),
            content: format!("use serde::{{Deserialize, Serialize}};\n\n{model_code}"),
        },
        GeneratedFile {
            relative_path: "Dockerfile".to_string(),
            content: dockerfile,
        },
        GeneratedFile {
            relative_path: "README.md".to_string(),
            content: readme,
        },
    ]
}

fn map_type_to_rust(t: &str) -> &str {
    match t.to_lowercase().as_str() {
        "string" | "str" | "text" => "String",
        "i32" | "int" | "integer" => "i32",
        "i64" | "bigint" | "long" => "i64",
        "f32" | "float" => "f32",
        "f64" | "double" => "f64",
        "bool" | "boolean" => "bool",
        "datetime" | "timestamp" => "chrono::NaiveDateTime",
        "uuid" => "uuid::Uuid",
        _ => "String",
    }
}

// ===========================================================================
// Python / FastAPI generator
// ===========================================================================

fn generate_python_fastapi(spec: &ScaffoldSpec) -> Vec<GeneratedFile> {
    let name = &spec.project_name;
    let desc = if spec.description.is_empty() {
        format!("{name} API")
    } else {
        spec.description.clone()
    };

    let db_driver = match spec.database.as_str() {
        "postgresql" => "postgresql+asyncpg",
        _ => "sqlite+aiosqlite",
    };
    let db_default_url = match spec.database.as_str() {
        "postgresql" => format!("postgresql+asyncpg://user:pass@localhost:5432/{name}"),
        _ => format!("sqlite+aiosqlite:///./{name}.db"),
    };

    let requirements = format!(
        r#"fastapi==0.111.0
uvicorn[standard]==0.30.1
sqlalchemy[asyncio]==2.0.30
pydantic==2.7.4
python-dotenv==1.0.1
alembic==1.13.1
{}
"#,
        match spec.database.as_str() {
            "postgresql" => "asyncpg==0.29.0",
            _ => "aiosqlite==0.20.0",
        }
    );

    // --- Pydantic models / SQLAlchemy models --------------------------------
    let mut pydantic_models = String::new();
    let mut sa_models = String::new();

    for model in &spec.models {
        // Pydantic schema
        pydantic_models.push_str(&format!("class {}Schema(BaseModel):\n", model.name));
        if model.fields.is_empty() {
            pydantic_models.push_str("    pass\n\n");
        } else {
            for field in &model.fields {
                let py_type = map_type_to_python(&field.field_type);
                pydantic_models.push_str(&format!("    {}: {}\n", field.name, py_type));
            }
            pydantic_models.push('\n');
        }

        // SQLAlchemy model
        sa_models.push_str(&format!(
            "class {name}(Base):\n    __tablename__ = \"{table}\"\n",
            name = model.name,
            table = model.name.to_lowercase() + "s",
        ));
        if model.fields.is_empty() {
            sa_models.push_str("    pass\n\n");
        } else {
            for field in &model.fields {
                let sa_type = map_type_to_sa_column(&field.field_type);
                let primary = if field.name == "id" {
                    ", primary_key=True"
                } else {
                    ""
                };
                sa_models.push_str(&format!(
                    "    {name} = Column({sa_type}{primary})\n",
                    name = field.name,
                ));
            }
            sa_models.push('\n');
        }
    }

    // --- Route handlers -----------------------------------------------------
    let mut route_code = String::new();
    for ep in &spec.endpoints {
        let method_lower = ep.method.to_lowercase();
        let fastapi_method = match method_lower.as_str() {
            "get" => "get",
            "post" => "post",
            "put" => "put",
            "delete" => "delete",
            "patch" => "patch",
            _ => "get",
        };
        // Convert :param to {param} for FastAPI
        let fastapi_path = ep
            .path
            .split('/')
            .map(|seg| {
                if let Some(stripped) = seg.strip_prefix(':') {
                    format!("{{{stripped}}}")
                } else {
                    seg.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("/");

        let comment = if ep.description.is_empty() {
            format!("Handler for {} {}", ep.method, ep.path)
        } else {
            ep.description.clone()
        };

        route_code.push_str(&format!(
            r#"@app.{fastapi_method}("{fastapi_path}")
async def {handler}():
    """{comment}"""
    # Add your business logic here
    return {{"handler": "{handler}", "message": "endpoint ready"}}


"#,
            handler = ep.handler,
        ));
    }

    let main_py = format!(
        r#""""
{desc}
"""
import os
import logging

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from sqlalchemy import Column, Integer, String, Float, Boolean, DateTime, create_engine
from sqlalchemy.orm import declarative_base

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Database
# ---------------------------------------------------------------------------

DATABASE_URL = os.getenv("DATABASE_URL", "{db_default_url}")
Base = declarative_base()

# ---------------------------------------------------------------------------
# SQLAlchemy Models
# ---------------------------------------------------------------------------

{sa_models}
# ---------------------------------------------------------------------------
# Pydantic Schemas
# ---------------------------------------------------------------------------

{pydantic_models}
# ---------------------------------------------------------------------------
# Application
# ---------------------------------------------------------------------------

app = FastAPI(title="{name}", description="{desc}")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

{route_code}
if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="0.0.0.0", port=3000, reload=True)
"#
    );

    let dockerfile = r#"FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
EXPOSE 3000
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "3000"]
"#
    .to_string();

    let readme = format!(
        r#"# {name}

{desc}

## Getting started

```bash
pip install -r requirements.txt
python main.py
```

The server will start on `http://localhost:3000`.

## Endpoints

{}

## Database

Backend: {db_driver}
"#,
        spec.endpoints
            .iter()
            .map(|e| format!("- `{} {}` - {}", e.method, e.path, e.description))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    vec![
        GeneratedFile {
            relative_path: "requirements.txt".to_string(),
            content: requirements,
        },
        GeneratedFile {
            relative_path: "main.py".to_string(),
            content: main_py,
        },
        GeneratedFile {
            relative_path: "models.py".to_string(),
            content: format!(
                "from sqlalchemy import Column, Integer, String, Float, Boolean, DateTime\nfrom sqlalchemy.orm import declarative_base\n\nBase = declarative_base()\n\n{sa_models}"
            ),
        },
        GeneratedFile {
            relative_path: "Dockerfile".to_string(),
            content: dockerfile,
        },
        GeneratedFile {
            relative_path: "README.md".to_string(),
            content: readme,
        },
    ]
}

fn map_type_to_python(t: &str) -> &str {
    match t.to_lowercase().as_str() {
        "string" | "str" | "text" => "str",
        "i32" | "int" | "integer" | "i64" | "bigint" | "long" => "int",
        "f32" | "float" | "f64" | "double" => "float",
        "bool" | "boolean" => "bool",
        "datetime" | "timestamp" => "str",
        "uuid" => "str",
        _ => "str",
    }
}

fn map_type_to_sa_column(t: &str) -> &str {
    match t.to_lowercase().as_str() {
        "string" | "str" | "text" | "uuid" => "String",
        "i32" | "int" | "integer" | "i64" | "bigint" | "long" => "Integer",
        "f32" | "float" | "f64" | "double" => "Float",
        "bool" | "boolean" => "Boolean",
        "datetime" | "timestamp" => "DateTime",
        _ => "String",
    }
}

// ===========================================================================
// Node.js / Express generator
// ===========================================================================

fn generate_node_express(spec: &ScaffoldSpec) -> Vec<GeneratedFile> {
    let name = &spec.project_name;
    let desc = if spec.description.is_empty() {
        format!("{name} API")
    } else {
        spec.description.clone()
    };

    let dialect = match spec.database.as_str() {
        "postgresql" => "postgres",
        _ => "sqlite",
    };
    let storage_line = if dialect == "sqlite" {
        format!(r#"  storage: "./{name}.sqlite","#)
    } else {
        String::new()
    };

    let package_json = format!(
        r#"{{
  "name": "{name}",
  "version": "1.0.0",
  "description": "{desc}",
  "main": "src/index.js",
  "scripts": {{
    "start": "node src/index.js",
    "dev": "node --watch src/index.js"
  }},
  "dependencies": {{
    "express": "^4.19.2",
    "sequelize": "^6.37.3",
    "cors": "^2.8.5",
    "helmet": "^7.1.0",
    "morgan": "^1.10.0",
    "dotenv": "^16.4.5"{extra_dep}
  }}
}}
"#,
        extra_dep = match spec.database.as_str() {
            "postgresql" => ",\n    \"pg\": \"^8.11.5\",\n    \"pg-hstore\": \"^2.3.4\"",
            _ => ",\n    \"sqlite3\": \"^5.1.7\"",
        }
    );

    // --- Sequelize models ---------------------------------------------------
    let mut model_defs = String::new();
    for model in &spec.models {
        model_defs.push_str(&format!(
            "const {name} = sequelize.define('{name}', {{\n",
            name = model.name,
        ));
        for field in &model.fields {
            if field.name == "id" {
                continue; // Sequelize auto-generates id
            }
            let seq_type = map_type_to_sequelize(&field.field_type);
            model_defs.push_str(&format!(
                "  {}: {{ type: DataTypes.{seq_type} }},\n",
                field.name
            ));
        }
        model_defs.push_str("});\n\n");
    }

    // --- Route handlers -----------------------------------------------------
    let mut route_code = String::new();
    for ep in &spec.endpoints {
        let method_lower = ep.method.to_lowercase();
        let express_method = match method_lower.as_str() {
            "get" => "get",
            "post" => "post",
            "put" => "put",
            "delete" => "delete",
            "patch" => "patch",
            _ => "get",
        };
        let comment = if ep.description.is_empty() {
            format!("{} {}", ep.method, ep.path)
        } else {
            ep.description.clone()
        };

        route_code.push_str(&format!(
            r#"// {comment}
app.{express_method}('{path}', async (req, res) => {{
  // Add your business logic here
  res.json({{ handler: '{handler}', message: 'endpoint ready' }});
}});

"#,
            path = ep.path,
            handler = ep.handler,
        ));
    }

    let index_js = format!(
        r#"'use strict';

require('dotenv').config();
const express = require('express');
const cors = require('cors');
const helmet = require('helmet');
const morgan = require('morgan');
const {{ Sequelize, DataTypes }} = require('sequelize');

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

const sequelize = new Sequelize({{
  dialect: '{dialect}',
{storage_line}
  logging: false,
}});

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

{model_defs}
// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

const app = express();

app.use(helmet());
app.use(cors());
app.use(morgan('combined'));
app.use(express.json());

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

{route_code}
// ---------------------------------------------------------------------------
// Start server
// ---------------------------------------------------------------------------

const PORT = process.env.PORT || 3000;

(async () => {{
  try {{
    await sequelize.authenticate();
    console.log('Database connected');
    await sequelize.sync();
    app.listen(PORT, () => {{
      console.log(`Server running on port ${{PORT}}`);
    }});
  }} catch (err) {{
    console.error('Failed to start:', err);
    process.exit(1);
  }}
}})();
"#
    );

    let dockerfile = r#"FROM node:20-slim
WORKDIR /app
COPY package*.json ./
RUN npm ci --only=production
COPY . .
EXPOSE 3000
CMD ["node", "src/index.js"]
"#
    .to_string();

    let readme = format!(
        r#"# {name}

{desc}

## Getting started

```bash
npm install
npm start
```

The server will start on `http://localhost:3000`.

## Endpoints

{}

## Database

Backend: {dialect}
"#,
        spec.endpoints
            .iter()
            .map(|e| format!("- `{} {}` - {}", e.method, e.path, e.description))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    vec![
        GeneratedFile {
            relative_path: "package.json".to_string(),
            content: package_json,
        },
        GeneratedFile {
            relative_path: "src/index.js".to_string(),
            content: index_js,
        },
        GeneratedFile {
            relative_path: "src/models.js".to_string(),
            content: format!(
                "'use strict';\n\nconst {{ Sequelize, DataTypes }} = require('sequelize');\n\n{model_defs}\nmodule.exports = {{}};\n"
            ),
        },
        GeneratedFile {
            relative_path: "Dockerfile".to_string(),
            content: dockerfile,
        },
        GeneratedFile {
            relative_path: "README.md".to_string(),
            content: readme,
        },
    ]
}

fn map_type_to_sequelize(t: &str) -> &str {
    match t.to_lowercase().as_str() {
        "string" | "str" | "text" | "uuid" => "STRING",
        "i32" | "int" | "integer" | "i64" | "bigint" | "long" => "INTEGER",
        "f32" | "float" | "f64" | "double" => "FLOAT",
        "bool" | "boolean" => "BOOLEAN",
        "datetime" | "timestamp" => "DATE",
        _ => "STRING",
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_spec_args(framework: &str) -> serde_json::Value {
        serde_json::json!({
            "action": "generate",
            "framework": framework,
            "project_name": "test_api",
            "description": "A test API project",
            "output_dir": "",  // will be replaced per test
            "endpoints": [
                {
                    "method": "GET",
                    "path": "/health",
                    "handler": "health_check",
                    "description": "Health check"
                },
                {
                    "method": "POST",
                    "path": "/items",
                    "handler": "create_item",
                    "description": "Create an item",
                    "request_body": { "name": "string" },
                    "response": { "id": "i64", "name": "string" }
                },
                {
                    "method": "GET",
                    "path": "/items/:id",
                    "handler": "get_item",
                    "description": "Get item by ID"
                }
            ],
            "models": [
                {
                    "name": "Item",
                    "fields": [
                        { "name": "id", "type": "i64" },
                        { "name": "name", "type": "String" },
                        { "name": "created_at", "type": "DateTime" }
                    ]
                }
            ],
            "database": "sqlite"
        })
    }

    fn make_call(args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "test_call".to_string(),
            name: "api_scaffold".to_string(),
            arguments: args,
        }
    }

    // -----------------------------------------------------------------------
    // list_frameworks
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_frameworks() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({ "action": "list_frameworks" }));
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let frameworks = parsed["frameworks"].as_array().unwrap();
        assert_eq!(frameworks.len(), 3);

        let keys: Vec<&str> = frameworks
            .iter()
            .map(|f| f["key"].as_str().unwrap())
            .collect();
        assert!(keys.contains(&"rust_axum"));
        assert!(keys.contains(&"python_fastapi"));
        assert!(keys.contains(&"node_express"));
    }

    // -----------------------------------------------------------------------
    // generate — Rust / Axum
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_rust_axum() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("rust_project");
        let out_str = out_dir.to_str().unwrap();

        let mut args = sample_spec_args("rust_axum");
        args["output_dir"] = serde_json::Value::String(out_str.to_string());

        let call = make_call(args);
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["framework"], "rust_axum");
        assert_eq!(parsed["project_name"], "test_api");

        // Verify generated files exist
        assert!(out_dir.join("Cargo.toml").exists());
        assert!(out_dir.join("src/main.rs").exists());
        assert!(out_dir.join("src/models.rs").exists());
        assert!(out_dir.join("Dockerfile").exists());
        assert!(out_dir.join("README.md").exists());

        // Verify content includes expected strings
        let cargo = tokio::fs::read_to_string(out_dir.join("Cargo.toml"))
            .await
            .unwrap();
        assert!(cargo.contains("axum"));
        assert!(cargo.contains("test_api"));
        assert!(cargo.contains("sqlx"));

        let main = tokio::fs::read_to_string(out_dir.join("src/main.rs"))
            .await
            .unwrap();
        assert!(main.contains("health_check"));
        assert!(main.contains("create_item"));
        assert!(main.contains("get_item"));
        assert!(main.contains("CorsLayer"));
        assert!(main.contains("TraceLayer"));
        assert!(main.contains("struct Item"));
    }

    // -----------------------------------------------------------------------
    // generate — Python / FastAPI
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_python_fastapi() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("python_project");
        let out_str = out_dir.to_str().unwrap();

        let mut args = sample_spec_args("python_fastapi");
        args["output_dir"] = serde_json::Value::String(out_str.to_string());

        let call = make_call(args);
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["framework"], "python_fastapi");

        // Verify generated files exist
        assert!(out_dir.join("requirements.txt").exists());
        assert!(out_dir.join("main.py").exists());
        assert!(out_dir.join("models.py").exists());
        assert!(out_dir.join("Dockerfile").exists());
        assert!(out_dir.join("README.md").exists());

        let reqs = tokio::fs::read_to_string(out_dir.join("requirements.txt"))
            .await
            .unwrap();
        assert!(reqs.contains("fastapi"));
        assert!(reqs.contains("uvicorn"));
        assert!(reqs.contains("sqlalchemy"));

        let main = tokio::fs::read_to_string(out_dir.join("main.py"))
            .await
            .unwrap();
        assert!(main.contains("health_check"));
        assert!(main.contains("create_item"));
        assert!(main.contains("CORSMiddleware"));
        assert!(main.contains("class ItemSchema"));
        assert!(main.contains("class Item"));
    }

    // -----------------------------------------------------------------------
    // generate — Node / Express
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_node_express() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("node_project");
        let out_str = out_dir.to_str().unwrap();

        let mut args = sample_spec_args("node_express");
        args["output_dir"] = serde_json::Value::String(out_str.to_string());

        let call = make_call(args);
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["framework"], "node_express");

        // Verify generated files exist
        assert!(out_dir.join("package.json").exists());
        assert!(out_dir.join("src/index.js").exists());
        assert!(out_dir.join("src/models.js").exists());
        assert!(out_dir.join("Dockerfile").exists());
        assert!(out_dir.join("README.md").exists());

        let pkg = tokio::fs::read_to_string(out_dir.join("package.json"))
            .await
            .unwrap();
        assert!(pkg.contains("express"));
        assert!(pkg.contains("sequelize"));
        assert!(pkg.contains("cors"));
        assert!(pkg.contains("helmet"));

        let index = tokio::fs::read_to_string(out_dir.join("src/index.js"))
            .await
            .unwrap();
        assert!(index.contains("health_check"));
        assert!(index.contains("create_item"));
        assert!(index.contains("helmet()"));
        assert!(index.contains("cors()"));
        assert!(index.contains("sequelize"));
    }

    // -----------------------------------------------------------------------
    // generated files exist in output directory
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generated_files_exist() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("files_check");
        let out_str = out_dir.to_str().unwrap();

        let mut args = sample_spec_args("rust_axum");
        args["output_dir"] = serde_json::Value::String(out_str.to_string());

        let call = make_call(args);
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let files = parsed["files_created"].as_array().unwrap();
        assert_eq!(files.len(), 5);

        for file_val in files {
            let rel_path = file_val.as_str().unwrap();
            let full_path = out_dir.join(rel_path);
            assert!(
                full_path.exists(),
                "Expected file to exist: {}",
                full_path.display()
            );
        }
    }

    // -----------------------------------------------------------------------
    // invalid framework
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_invalid_framework_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_str = dir.path().to_str().unwrap();

        let call = make_call(serde_json::json!({
            "action": "generate",
            "framework": "ruby_sinatra",
            "project_name": "test",
            "output_dir": out_str,
        }));

        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error, "Expected error for invalid framework");
        assert!(result.content.contains("Invalid framework"));
        assert!(result.content.contains("ruby_sinatra"));
    }

    // -----------------------------------------------------------------------
    // missing required fields
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_missing_action_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({}));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("action"));
    }

    #[tokio::test]
    async fn test_missing_framework_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({
            "action": "generate",
            "project_name": "test",
            "output_dir": "/tmp/test"
        }));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("framework"));
    }

    #[tokio::test]
    async fn test_missing_project_name_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({
            "action": "generate",
            "framework": "rust_axum",
            "output_dir": "/tmp/test"
        }));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("project_name"));
    }

    #[tokio::test]
    async fn test_missing_output_dir_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({
            "action": "generate",
            "framework": "rust_axum",
            "project_name": "test"
        }));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("output_dir"));
    }

    #[tokio::test]
    async fn test_relative_output_dir_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({
            "action": "generate",
            "framework": "rust_axum",
            "project_name": "test",
            "output_dir": "relative/path"
        }));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("absolute"));
    }

    // -----------------------------------------------------------------------
    // unknown action
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_unknown_action_returns_error() {
        let skill = ApiScaffoldSkill::new();
        let call = make_call(serde_json::json!({ "action": "destroy" }));
        let result = skill.execute(call).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Unknown action"));
    }

    // -----------------------------------------------------------------------
    // descriptor
    // -----------------------------------------------------------------------

    #[test]
    fn test_descriptor() {
        let skill = ApiScaffoldSkill::new();
        let desc = skill.descriptor();
        assert_eq!(desc.name, "api_scaffold");
        assert!(!desc.description.is_empty());
        assert!(!desc.required_capabilities.is_empty());
    }

    // -----------------------------------------------------------------------
    // validate_arguments
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_arguments_denies_disallowed_path() {
        let skill = ApiScaffoldSkill::new();
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileWrite {
            allowed_paths: vec!["/allowed".to_string()],
        });

        let call = make_call(serde_json::json!({
            "action": "generate",
            "output_dir": "/tmp/some_dir"
        }));
        let result = skill.validate_arguments(&call, &perms);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_arguments_allows_permitted_path() {
        let skill = ApiScaffoldSkill::new();
        let mut perms = PermissionSet::new();
        perms.grant(Capability::FileWrite {
            allowed_paths: vec!["/tmp".to_string()],
        });

        let call = make_call(serde_json::json!({
            "action": "generate",
            "output_dir": "/tmp/some_dir"
        }));
        let result = skill.validate_arguments(&call, &perms);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_arguments_skips_for_list() {
        let skill = ApiScaffoldSkill::new();
        let perms = PermissionSet::new(); // empty — would deny writes

        let call = make_call(serde_json::json!({ "action": "list_frameworks" }));
        let result = skill.validate_arguments(&call, &perms);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // postgresql variant
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_rust_axum_postgresql() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("pg_project");
        let out_str = out_dir.to_str().unwrap();

        let mut args = sample_spec_args("rust_axum");
        args["output_dir"] = serde_json::Value::String(out_str.to_string());
        args["database"] = serde_json::Value::String("postgresql".to_string());

        let call = make_call(args);
        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);

        let cargo = tokio::fs::read_to_string(out_dir.join("Cargo.toml"))
            .await
            .unwrap();
        assert!(cargo.contains("postgres"));
    }

    // -----------------------------------------------------------------------
    // empty endpoints / models
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_with_no_endpoints() {
        let skill = ApiScaffoldSkill::new();
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("empty_project");
        let out_str = out_dir.to_str().unwrap();

        let call = make_call(serde_json::json!({
            "action": "generate",
            "framework": "node_express",
            "project_name": "empty_api",
            "output_dir": out_str,
        }));

        let result = skill.execute(call).await.unwrap();
        assert!(!result.is_error, "Result: {}", result.content);
        assert!(out_dir.join("package.json").exists());
        assert!(out_dir.join("src/index.js").exists());
    }

    // -----------------------------------------------------------------------
    // Default trait
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_trait() {
        let skill = ApiScaffoldSkill::default();
        assert_eq!(skill.descriptor().name, "api_scaffold");
    }
}
