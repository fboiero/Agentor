# MCP Server Registry

> A curated catalog of Model Context Protocol (MCP) servers that integrate out-of-the-box with Argentor. Because Argentor speaks vanilla MCP (JSON-RPC 2.0), any of the 5,800+ public MCP servers in the ecosystem can be plugged in without writing Rust code.

**Last updated:** April 2026

---

## How this registry works

Every entry in this document gives you:

1. **Name** — the common identifier for the server
2. **Repo / URL** — where the source lives
3. **Description** — one line on what it does
4. **Argentor connection snippet** — paste this into your `argentor.toml`
5. **Auth** — what credentials you need (if any)

Two ways to wire a server into Argentor:

### Static — via `argentor.toml`

```toml
[mcp.servers.<alias>]
command = "<binary>"
args = ["<arg1>", "<arg2>"]
env = { KEY = "${ENV_VAR}" }   # optional
```

At boot, Argentor spawns each configured server, runs the handshake, discovers tools, and registers them as skills.

### Dynamic — via Rust code

```rust
use argentor_mcp::{McpClient, McpSkill};
use argentor_skills::SkillRegistry;
use std::sync::Arc;

let (client, tools) = McpClient::connect(
    "npx",
    &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    &[],
).await?;

let client = Arc::new(client);
let mut registry = SkillRegistry::new();
for tool in tools {
    registry.register(Arc::new(McpSkill::new(client.clone(), tool)));
}
```

See [MCP_INTEGRATION_GUIDE.md](./MCP_INTEGRATION_GUIDE.md) for the full integration flow.

---

## Auth legend

| Symbol | Meaning |
|--------|---------|
| **None** | No authentication required |
| **API key** | Single API key / bearer token |
| **OAuth** | OAuth2 flow (usually refresh-token based) |
| **Local** | Operates on local resources only — no remote auth |
| **Custom** | Server-specific credential scheme (service account, JSON key file, etc.) |

> Credentials are best managed through Argentor's [Credential Vault](./MCP_INTEGRATION_GUIDE.md#7-credential-vault-for-mcp-api-keys) (AES-256-GCM encrypted, per-provider quotas, automatic rotation) instead of raw env vars.

---

## 1. Filesystem and Local Tools

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| filesystem | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) | Read, write, search files within configured directories | Local |
| git | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/git) | Read commit history, diffs, branches on a local repo | Local |
| sqlite | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/sqlite) | Query and modify a SQLite database file | Local |
| memory | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/memory) | Persistent key-value memory across sessions | Local |
| time | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/time) | Timezone-aware time queries and conversions | None |
| fetch | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/fetch) | Fetch URLs and convert to Markdown | None |
| everything | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/everything) | Reference/demo server covering all MCP primitives | None |
| sequential-thinking | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking) | Structured multi-step reasoning scaffold | None |

```toml
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/Users/me/projects"]

[mcp.servers.git]
command = "uvx"
args = ["mcp-server-git", "--repository", "/Users/me/projects/repo"]

[mcp.servers.sqlite]
command = "uvx"
args = ["mcp-server-sqlite", "--db-path", "/Users/me/data/app.db"]

[mcp.servers.memory]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-memory"]

[mcp.servers.time]
command = "uvx"
args = ["mcp-server-time"]

[mcp.servers.fetch]
command = "uvx"
args = ["mcp-server-fetch"]

[mcp.servers.sequential_thinking]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-sequential-thinking"]
```

---

## 2. Cloud Services

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| aws-core | [awslabs/mcp](https://github.com/awslabs/mcp) | Core AWS operations via boto3 (EC2, S3, IAM, etc.) | API key (AWS credentials) |
| aws-cdk | [awslabs/mcp](https://github.com/awslabs/mcp) | Generate and deploy AWS CDK infrastructure | API key |
| aws-terraform | [awslabs/mcp](https://github.com/awslabs/mcp) | AWS Terraform module generation | API key |
| aws-cost-analysis | [awslabs/mcp](https://github.com/awslabs/mcp) | Query AWS Cost Explorer | API key |
| gcp-mcp | [eniayomi/gcp-mcp](https://github.com/eniayomi/gcp-mcp) | Google Cloud Platform operations | Custom (service account JSON) |
| azure-mcp | [Azure/azure-mcp](https://github.com/Azure/azure-mcp) | Azure resource management | OAuth |
| cloudflare | [cloudflare/mcp-server-cloudflare](https://github.com/cloudflare/mcp-server-cloudflare) | Manage Cloudflare Workers, KV, R2, DNS | API key |
| digitalocean-mcp | [digitalocean/digitalocean-mcp](https://github.com/digitalocean/digitalocean-mcp) | DigitalOcean droplets, Spaces, databases | API key |
| heroku-mcp | [heroku/heroku-mcp-server](https://github.com/heroku/heroku-mcp-server) | Heroku apps, dynos, add-ons | API key |
| vercel-mcp | [vercel/mcp](https://vercel.com/docs/mcp) | Vercel deployments and projects | API key |
| fly-mcp | Community | Fly.io apps and machines | API key |

```toml
[mcp.servers.aws_core]
command = "uvx"
args = ["awslabs.core-mcp-server@latest"]
env = { AWS_PROFILE = "default", AWS_REGION = "us-east-1" }

[mcp.servers.gcp]
command = "uvx"
args = ["gcp-mcp"]
env = { GOOGLE_APPLICATION_CREDENTIALS = "/path/to/sa.json" }

[mcp.servers.azure]
command = "npx"
args = ["-y", "@azure/mcp@latest", "server", "start"]

[mcp.servers.cloudflare]
command = "npx"
args = ["-y", "@cloudflare/mcp-server-cloudflare"]
env = { CLOUDFLARE_API_TOKEN = "${CF_API_TOKEN}" }

[mcp.servers.digitalocean]
command = "npx"
args = ["-y", "@digitalocean/mcp"]
env = { DIGITALOCEAN_API_TOKEN = "${DO_TOKEN}" }

[mcp.servers.vercel]
command = "npx"
args = ["-y", "@vercel/mcp-adapter"]
env = { VERCEL_TOKEN = "${VERCEL_TOKEN}" }
```

---

## 3. Databases

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| postgres | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/postgres) | Read-only Postgres queries with schema introspection | API key (connection string) |
| mysql-mcp | [benborla/mcp-server-mysql](https://github.com/benborla/mcp-server-mysql) | Query MySQL/MariaDB with schema awareness | API key |
| mongodb-mcp | [mongodb-js/mongodb-mcp-server](https://github.com/mongodb-js/mongodb-mcp-server) | MongoDB CRUD and aggregation pipelines | API key |
| redis | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/redis) | Redis GET/SET/HGET/etc. | API key (optional) |
| clickhouse-mcp | [ClickHouse/mcp-clickhouse](https://github.com/ClickHouse/mcp-clickhouse) | ClickHouse analytic queries | API key |
| bigquery-mcp | [LucasHild/mcp-server-bigquery](https://github.com/LucasHild/mcp-server-bigquery) | Google BigQuery datasets and queries | Custom (service account) |
| snowflake-mcp | [isaacwasserman/mcp-snowflake-server](https://github.com/isaacwasserman/mcp-snowflake-server) | Snowflake warehouse queries | API key |
| supabase-mcp | [supabase-community/supabase-mcp](https://github.com/supabase-community/supabase-mcp) | Supabase tables, RPCs, auth | API key |
| neon-mcp | [neondatabase/mcp-server-neon](https://github.com/neondatabase/mcp-server-neon) | Neon serverless Postgres | API key |
| pinecone-mcp | [sirmews/mcp-pinecone](https://github.com/sirmews/mcp-pinecone) | Pinecone vector DB (upsert/query) | API key |
| qdrant-mcp | [qdrant/mcp-server-qdrant](https://github.com/qdrant/mcp-server-qdrant) | Qdrant vector search | API key |
| elasticsearch-mcp | [cr7258/elasticsearch-mcp-server](https://github.com/cr7258/elasticsearch-mcp-server) | Elasticsearch / OpenSearch | API key |
| duckdb-mcp | [ktanaka101/mcp-server-duckdb](https://github.com/ktanaka101/mcp-server-duckdb) | Embedded OLAP queries over files | Local |

```toml
[mcp.servers.postgres]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres", "${POSTGRES_URL}"]

[mcp.servers.mysql]
command = "npx"
args = ["-y", "@benborla29/mcp-server-mysql"]
env = { MYSQL_HOST = "localhost", MYSQL_USER = "root", MYSQL_PASSWORD = "${MYSQL_PW}", MYSQL_DB = "app" }

[mcp.servers.mongodb]
command = "npx"
args = ["-y", "mongodb-mcp-server"]
env = { MDB_MCP_CONNECTION_STRING = "${MONGO_URL}" }

[mcp.servers.redis]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-redis", "redis://localhost:6379"]

[mcp.servers.clickhouse]
command = "uvx"
args = ["mcp-clickhouse"]
env = { CLICKHOUSE_HOST = "localhost", CLICKHOUSE_USER = "default", CLICKHOUSE_PASSWORD = "${CH_PW}" }

[mcp.servers.supabase]
command = "npx"
args = ["-y", "@supabase/mcp-server-supabase@latest"]
env = { SUPABASE_ACCESS_TOKEN = "${SUPABASE_TOKEN}" }

[mcp.servers.qdrant]
command = "uvx"
args = ["mcp-server-qdrant"]
env = { QDRANT_URL = "http://localhost:6333", COLLECTION_NAME = "docs" }
```

---

## 4. Communication

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| slack | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/slack) | Post messages, list channels, read history | API key (bot token) |
| discord-mcp | [v-3/discordmcp](https://github.com/v-3/discordmcp) | Discord messaging and channel management | API key (bot token) |
| telegram-mcp | [chigwell/telegram-mcp](https://github.com/chigwell/telegram-mcp) | Telegram Bot API | API key |
| email-mcp (mcp-email-server) | [ai-zerolab/mcp-email-server](https://github.com/ai-zerolab/mcp-email-server) | IMAP/SMTP send and read | API key (SMTP creds) |
| gmail-mcp | [GongRzhe/Gmail-MCP-Server](https://github.com/GongRzhe/Gmail-MCP-Server) | Gmail read, send, search, labels | OAuth |
| whatsapp-mcp | [lharries/whatsapp-mcp](https://github.com/lharries/whatsapp-mcp) | WhatsApp messaging via whatsmeow | Custom |
| twilio-mcp | [twilio-labs/mcp](https://github.com/twilio-labs/mcp) | SMS, voice, and WhatsApp via Twilio | API key |
| sendgrid-mcp | Community | Transactional email via SendGrid | API key |
| signal-mcp | Community | Signal messenger bridge | Custom |
| matrix-mcp | Community | Matrix chat protocol | API key |
| zoom-mcp | [zoom/mcp-server-zoom](https://github.com/zoom/mcp-server-zoom) | Zoom meetings, recordings, chat | OAuth |
| msteams-mcp | [microsoft/mcp](https://github.com/microsoft/mcp) | Microsoft Teams messaging | OAuth |

```toml
[mcp.servers.slack]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-slack"]
env = { SLACK_BOT_TOKEN = "${SLACK_BOT_TOKEN}", SLACK_TEAM_ID = "${SLACK_TEAM}" }

[mcp.servers.discord]
command = "npx"
args = ["-y", "@v-3/discordmcp"]
env = { DISCORD_TOKEN = "${DISCORD_TOKEN}" }

[mcp.servers.telegram]
command = "uvx"
args = ["telegram-mcp"]
env = { TELEGRAM_BOT_TOKEN = "${TG_BOT_TOKEN}" }

[mcp.servers.gmail]
command = "npx"
args = ["-y", "@gongrzhe/server-gmail-autoauth-mcp"]

[mcp.servers.twilio]
command = "npx"
args = ["-y", "@twilio-alpha/mcp"]
env = { TWILIO_ACCOUNT_SID = "${TWILIO_SID}", TWILIO_AUTH_TOKEN = "${TWILIO_TOKEN}" }
```

---

## 5. Productivity

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| github | [github/github-mcp-server](https://github.com/github/github-mcp-server) | Issues, PRs, repos, Actions, code search (official) | API key (PAT) |
| gitlab | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/gitlab) | GitLab issues and merge requests | API key |
| jira | [sooperset/mcp-atlassian](https://github.com/sooperset/mcp-atlassian) | Jira issues, projects, JQL search | API key |
| confluence | [sooperset/mcp-atlassian](https://github.com/sooperset/mcp-atlassian) | Confluence pages and spaces | API key |
| linear-mcp | [jerhadf/linear-mcp-server](https://github.com/jerhadf/linear-mcp-server) | Linear issues and projects | API key |
| notion-mcp | [makenotion/notion-mcp-server](https://github.com/makenotion/notion-mcp-server) | Notion pages, databases, blocks (official) | API key |
| asana-mcp | [asana/asana-mcp-server](https://github.com/asana/asana-mcp-server) | Asana tasks and projects | API key |
| clickup-mcp | [taazkareem/clickup-mcp-server](https://github.com/taazkareem/clickup-mcp-server) | ClickUp tasks | API key |
| trello-mcp | [delorenj/mcp-server-trello](https://github.com/delorenj/mcp-server-trello) | Trello boards, lists, cards | API key |
| airtable-mcp | [felores/airtable-mcp](https://github.com/felores/airtable-mcp) | Airtable bases and records | API key |
| google-drive | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/gdrive) | Google Drive files and folders | OAuth |
| google-calendar | [nspady/google-calendar-mcp](https://github.com/nspady/google-calendar-mcp) | Calendar events and schedules | OAuth |
| obsidian-mcp | [StevenStavrakis/obsidian-mcp](https://github.com/StevenStavrakis/obsidian-mcp) | Obsidian vault notes | Local |
| todoist-mcp | [abhiz123/todoist-mcp-server](https://github.com/abhiz123/todoist-mcp-server) | Todoist tasks | API key |
| raycast-mcp | Community | Raycast quick actions | Local |

```toml
[mcp.servers.github]
command = "docker"
args = ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }

[mcp.servers.jira]
command = "uvx"
args = ["mcp-atlassian"]
env = { CONFLUENCE_URL = "https://company.atlassian.net/wiki", JIRA_URL = "https://company.atlassian.net", JIRA_USERNAME = "${JIRA_USER}", JIRA_API_TOKEN = "${JIRA_TOKEN}" }

[mcp.servers.linear]
command = "npx"
args = ["-y", "@jerhadf/linear-mcp-server"]
env = { LINEAR_API_KEY = "${LINEAR_KEY}" }

[mcp.servers.notion]
command = "npx"
args = ["-y", "@notionhq/notion-mcp-server"]
env = { OPENAPI_MCP_HEADERS = '{"Authorization":"Bearer ${NOTION_TOKEN}","Notion-Version":"2022-06-28"}' }

[mcp.servers.asana]
command = "npx"
args = ["-y", "@roychri/mcp-server-asana"]
env = { ASANA_ACCESS_TOKEN = "${ASANA_TOKEN}" }

[mcp.servers.airtable]
command = "npx"
args = ["-y", "airtable-mcp-server"]
env = { AIRTABLE_API_KEY = "${AIRTABLE_KEY}" }
```

---

## 6. Search and Knowledge

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| brave-search | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/brave-search) | Web + local search via Brave Search API | API key |
| perplexity-mcp | [ppl-ai/modelcontextprotocol](https://github.com/ppl-ai/modelcontextprotocol) | Perplexity answer engine | API key |
| tavily-mcp | [tavily-ai/tavily-mcp](https://github.com/tavily-ai/tavily-mcp) | Tavily search + content extraction (AI-optimized) | API key |
| exa-mcp | [exa-labs/exa-mcp-server](https://github.com/exa-labs/exa-mcp-server) | Exa neural search + research | API key |
| wikipedia-mcp | [Rudra-ravi/wikipedia-mcp](https://github.com/Rudra-ravi/wikipedia-mcp) | Wikipedia article search and fetch | None |
| arxiv-mcp | [blazickjp/arxiv-mcp-server](https://github.com/blazickjp/arxiv-mcp-server) | arXiv paper search and download | None |
| google-scholar-mcp | Community | Google Scholar publication search | None |
| pubmed-mcp | Community | PubMed biomedical literature | None |
| duckduckgo-mcp | [nickclyde/duckduckgo-mcp-server](https://github.com/nickclyde/duckduckgo-mcp-server) | DuckDuckGo web search (no key) | None |
| kagi-mcp | [kagisearch/kagimcp](https://github.com/kagisearch/kagimcp) | Kagi premium search | API key |
| youtube-transcript-mcp | [kimtaeyoon83/mcp-server-youtube-transcript](https://github.com/kimtaeyoon83/mcp-server-youtube-transcript) | Fetch YouTube transcripts | None |
| hackernews-mcp | Community | Hacker News stories and comments | None |

```toml
[mcp.servers.brave_search]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-brave-search"]
env = { BRAVE_API_KEY = "${BRAVE_API_KEY}" }

[mcp.servers.perplexity]
command = "npx"
args = ["-y", "server-perplexity-ask"]
env = { PERPLEXITY_API_KEY = "${PPLX_KEY}" }

[mcp.servers.tavily]
command = "npx"
args = ["-y", "tavily-mcp@latest"]
env = { TAVILY_API_KEY = "${TAVILY_KEY}" }

[mcp.servers.exa]
command = "npx"
args = ["-y", "exa-mcp-server"]
env = { EXA_API_KEY = "${EXA_KEY}" }

[mcp.servers.duckduckgo]
command = "uvx"
args = ["duckduckgo-mcp-server"]

[mcp.servers.arxiv]
command = "uvx"
args = ["arxiv-mcp-server"]

[mcp.servers.wikipedia]
command = "uvx"
args = ["wikipedia-mcp"]
```

---

## 7. AI / ML Tools

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| openai-mcp | [pierrebrunelle/mcp-server-openai](https://github.com/pierrebrunelle/mcp-server-openai) | Direct OpenAI API access (completions, embeddings, DALL-E) | API key |
| huggingface-mcp | [shreyaskarnik/huggingface-mcp-server](https://github.com/shreyaskarnik/huggingface-mcp-server) | Hugging Face models, datasets, spaces | API key |
| replicate-mcp | [deepfates/mcp-replicate](https://github.com/deepfates/mcp-replicate) | Replicate model inference (text, image, video) | API key |
| elevenlabs-mcp | [elevenlabs/elevenlabs-mcp](https://github.com/elevenlabs/elevenlabs-mcp) | Text-to-speech and voice cloning | API key |
| stability-mcp | Community | Stability AI image generation | API key |
| cohere-mcp | Community | Cohere rerank and embed | API key |
| anthropic-mcp | Community | Anthropic Claude via MCP (bridge) | API key |
| gemini-mcp | Community | Google Gemini API | API key |
| langchain-bridge | Community | Expose LangChain tools as MCP | None |
| llamaindex-bridge | Community | Expose LlamaIndex retrievers as MCP | None |

```toml
[mcp.servers.openai]
command = "uvx"
args = ["mcp-server-openai"]
env = { OPENAI_API_KEY = "${OPENAI_KEY}" }

[mcp.servers.huggingface]
command = "uvx"
args = ["huggingface-mcp-server"]
env = { HF_TOKEN = "${HF_TOKEN}" }

[mcp.servers.replicate]
command = "npx"
args = ["-y", "mcp-replicate"]
env = { REPLICATE_API_TOKEN = "${REPLICATE_TOKEN}" }

[mcp.servers.elevenlabs]
command = "uvx"
args = ["elevenlabs-mcp"]
env = { ELEVENLABS_API_KEY = "${ELEVEN_KEY}" }
```

---

## 8. Web / Scraping

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| puppeteer | [modelcontextprotocol/servers-archived](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/puppeteer) | Headless Chrome automation | None |
| playwright-mcp | [microsoft/playwright-mcp](https://github.com/microsoft/playwright-mcp) | Microsoft Playwright browser automation (official) | None |
| browserbase-mcp | [browserbase/mcp-server-browserbase](https://github.com/browserbase/mcp-server-browserbase) | Cloud browser automation | API key |
| firecrawl-mcp | [mendableai/firecrawl-mcp-server](https://github.com/mendableai/firecrawl-mcp-server) | Firecrawl scrape + crawl + extract | API key |
| apify-mcp | [apify/actors-mcp-server](https://github.com/apify/actors-mcp-server) | Apify actors (scrapers marketplace) | API key |
| scrapingbee-mcp | Community | ScrapingBee proxy scraping | API key |
| fetch | [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers/tree/main/src/fetch) | Simple HTTP fetch with Markdown conversion | None |
| readability-mcp | Community | Extract readable content from URLs | None |
| selenium-mcp | Community | Selenium browser driver | None |
| bright-data-mcp | [luminati-io/brightdata-mcp](https://github.com/luminati-io/brightdata-mcp) | Bright Data proxies and web unblocker | API key |

```toml
[mcp.servers.puppeteer]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-puppeteer"]

[mcp.servers.playwright]
command = "npx"
args = ["-y", "@playwright/mcp@latest"]

[mcp.servers.browserbase]
command = "npx"
args = ["-y", "@browserbasehq/mcp-server-browserbase"]
env = { BROWSERBASE_API_KEY = "${BB_KEY}", BROWSERBASE_PROJECT_ID = "${BB_PROJECT}" }

[mcp.servers.firecrawl]
command = "npx"
args = ["-y", "firecrawl-mcp"]
env = { FIRECRAWL_API_KEY = "${FIRECRAWL_KEY}" }

[mcp.servers.apify]
command = "npx"
args = ["-y", "@apify/actors-mcp-server"]
env = { APIFY_TOKEN = "${APIFY_TOKEN}" }

[mcp.servers.fetch]
command = "uvx"
args = ["mcp-server-fetch"]
```

---

## 9. Developer Tools

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| docker-mcp | [ckreiling/mcp-server-docker](https://github.com/ckreiling/mcp-server-docker) | Manage containers, images, volumes, networks | Local |
| kubernetes-mcp | [Flux159/mcp-server-kubernetes](https://github.com/Flux159/mcp-server-kubernetes) | kubectl-equivalent operations | Custom (kubeconfig) |
| terraform-mcp | [hashicorp/terraform-mcp-server](https://github.com/hashicorp/terraform-mcp-server) | Terraform Registry provider/module discovery (official) | None |
| pulumi-mcp | [pulumi/mcp-server](https://github.com/pulumi/mcp-server) | Pulumi IaC operations | API key |
| ansible-mcp | Community | Ansible playbooks and inventory | Local |
| helm-mcp | Community | Helm chart search and install | Local |
| sentry-mcp | [getsentry/sentry-mcp](https://github.com/getsentry/sentry-mcp) | Sentry issues, events, releases | API key |
| datadog-mcp | [GeLi2001/datadog-mcp-server](https://github.com/GeLi2001/datadog-mcp-server) | Datadog metrics, logs, monitors | API key |
| grafana-mcp | [grafana/mcp-grafana](https://github.com/grafana/mcp-grafana) | Grafana dashboards and queries (official) | API key |
| prometheus-mcp | Community | Prometheus PromQL queries | None |
| honeycomb-mcp | [honeycombio/honeycomb-mcp](https://github.com/honeycombio/honeycomb-mcp) | Honeycomb observability queries | API key |
| vault-mcp | Community | HashiCorp Vault secret management | API key |
| circleci-mcp | [CircleCI-Public/mcp-server-circleci](https://github.com/CircleCI-Public/mcp-server-circleci) | CircleCI pipelines and builds | API key |
| npm-mcp | Community | npm package search and metadata | None |

```toml
[mcp.servers.docker]
command = "uvx"
args = ["mcp-server-docker"]

[mcp.servers.kubernetes]
command = "npx"
args = ["-y", "mcp-server-kubernetes"]

[mcp.servers.terraform]
command = "docker"
args = ["run", "-i", "--rm", "hashicorp/terraform-mcp-server"]

[mcp.servers.pulumi]
command = "npx"
args = ["-y", "@pulumi/mcp-server"]
env = { PULUMI_ACCESS_TOKEN = "${PULUMI_TOKEN}" }

[mcp.servers.sentry]
command = "uvx"
args = ["mcp-server-sentry"]
env = { SENTRY_AUTH_TOKEN = "${SENTRY_TOKEN}" }

[mcp.servers.grafana]
command = "uvx"
args = ["mcp-grafana"]
env = { GRAFANA_URL = "${GRAFANA_URL}", GRAFANA_API_KEY = "${GRAFANA_KEY}" }

[mcp.servers.circleci]
command = "npx"
args = ["-y", "@circleci/mcp-server-circleci"]
env = { CIRCLECI_TOKEN = "${CIRCLECI_TOKEN}" }
```

---

## 10. Finance

| Server | Repo | Description | Auth |
|--------|------|-------------|------|
| stripe-mcp | [stripe/agent-toolkit](https://github.com/stripe/agent-toolkit) | Stripe payments, customers, invoices (official) | API key |
| quickbooks-mcp | Community | QuickBooks Online accounting | OAuth |
| plaid-mcp | Community | Plaid bank account aggregation | API key |
| coinmarketcap-mcp | Community | Crypto market data | API key |
| alpaca-mcp | [alpacahq/alpaca-mcp-server](https://github.com/alpacahq/alpaca-mcp-server) | Stock trading via Alpaca (official) | API key |
| yahoo-finance-mcp | Community | Yahoo Finance market data | None |
| alphavantage-mcp | Community | Alpha Vantage financial data | API key |
| xero-mcp | Community | Xero accounting | OAuth |

```toml
[mcp.servers.stripe]
command = "npx"
args = ["-y", "@stripe/mcp", "--tools=all"]
env = { STRIPE_SECRET_KEY = "${STRIPE_SECRET}" }

[mcp.servers.alpaca]
command = "uvx"
args = ["alpaca-mcp-server"]
env = { ALPACA_API_KEY_ID = "${ALPACA_KEY}", ALPACA_API_SECRET_KEY = "${ALPACA_SECRET}" }

[mcp.servers.plaid]
command = "npx"
args = ["-y", "mcp-plaid"]
env = { PLAID_CLIENT_ID = "${PLAID_ID}", PLAID_SECRET = "${PLAID_SECRET}" }

[mcp.servers.coinmarketcap]
command = "uvx"
args = ["mcp-coinmarketcap"]
env = { COINMARKETCAP_API_KEY = "${CMC_KEY}" }
```

---

## Bonus: Reference / Curated Lists

The MCP ecosystem is growing weekly. To keep up with the 5,800+ server count:

- **Official servers index** — [modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers) (reference implementations by Anthropic)
- **Awesome MCP Servers** — [punkpeye/awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers) (the canonical community catalog)
- **MCP.so** — [mcp.so](https://mcp.so) (searchable directory with filters)
- **Smithery** — [smithery.ai](https://smithery.ai) (MCP server marketplace with one-click install)
- **Glama MCP** — [glama.ai/mcp/servers](https://glama.ai/mcp/servers) (analytics on popularity, quality)
- **Official registry** — [modelcontextprotocol.io/registry](https://modelcontextprotocol.io/registry) (standards body listing)

---

## Contributing to this registry

If you have deployed a public MCP server and want to add it to this catalog, open a PR to `docs/MCP_REGISTRY.md` with:

1. Category (one of the 10 above, or propose a new one)
2. Row entry: `name | repo | description | auth`
3. TOML snippet in the code block for that category

Please verify the snippet starts successfully against a current Argentor build before submitting.

---

**Next:** [MCP_INTEGRATION_GUIDE.md](./MCP_INTEGRATION_GUIDE.md) walks you through connecting these servers end-to-end, credential vault integration, debugging, and production patterns.
