# Setting up argentor-langchain-bridge (Option B)

> Step-by-step guide to create the `argentor-langchain-bridge` Python project as a SEPARATE repository.
> This bridge exposes LangChain tools as MCP servers, making them accessible to Argentor (and any MCP client).

## Why a separate repo?

| Reason | Detail |
|--------|--------|
| **Different language** | Python vs Rust — different toolchains, CI, tests |
| **Different audience** | Python/LangChain developers, not Rust |
| **Independent releases** | PyPI cadence (Python releases) vs crates.io (Rust) |
| **Lighter Argentor repo** | Don't pull Python deps into the main Rust project |
| **Consistent licensing** | Same AGPL-3.0-only as main Argentor project |

## Step 1 — Create the GitHub repository

```bash
gh repo create fboiero/argentor-langchain-bridge \
  --public \
  --description "MCP server that exposes LangChain tools to Argentor and any MCP-compatible client" \
  --license agpl-3.0
```

## Step 2 — Clone and scaffold

```bash
gh repo clone fboiero/argentor-langchain-bridge
cd argentor-langchain-bridge
```

Create this structure:

```
argentor-langchain-bridge/
├── pyproject.toml
├── README.md
├── LICENSE
├── .gitignore
├── .github/
│   └── workflows/
│       ├── test.yml
│       └── publish.yml
├── src/
│   └── argentor_langchain_bridge/
│       ├── __init__.py
│       ├── server.py            ← Main MCP server entry point
│       ├── registry.py          ← Tool discovery from langchain.tools
│       ├── tools/
│       │   ├── __init__.py
│       │   ├── search.py        ← SerpAPI, Google, etc.
│       │   ├── database.py      ← SQL, vector DBs
│       │   ├── filesystem.py    ← ReadFile, WriteFile
│       │   ├── web.py           ← Requests, BS4
│       │   └── code.py          ← Python REPL, Shell
│       └── adapters/
│           ├── __init__.py
│           └── mcp_adapter.py   ← LangChain Tool → MCP Tool conversion
└── tests/
    ├── __init__.py
    ├── test_server.py
    └── test_tools/
```

## Step 3 — pyproject.toml

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "argentor-langchain-bridge"
version = "0.1.0"
description = "MCP server exposing LangChain tools to Argentor and MCP-compatible clients"
authors = [{name = "Argentor Contributors"}]
license = "AGPL-3.0-only"
readme = "README.md"
requires-python = ">=3.10"
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: GNU Affero General Public License v3",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Topic :: Software Development :: Libraries",
]
dependencies = [
    "mcp>=1.0",
    "langchain>=0.3",
    "langchain-community>=0.3",
    "pydantic>=2.0",
]

[project.optional-dependencies]
all = [
    "langchain-openai",
    "langchain-anthropic",
    "langchain-pinecone",
    "langchain-postgres",
]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.23",
    "ruff>=0.1",
    "mypy>=1.0",
]

[project.scripts]
argentor-lc-bridge = "argentor_langchain_bridge.server:main"

[project.urls]
Homepage = "https://github.com/fboiero/argentor-langchain-bridge"
Documentation = "https://github.com/fboiero/argentor-langchain-bridge#readme"
Repository = "https://github.com/fboiero/argentor-langchain-bridge"
Issues = "https://github.com/fboiero/argentor-langchain-bridge/issues"
```

## Step 4 — Core server.py

```python
"""MCP server that exposes LangChain tools."""
import asyncio
import logging
from typing import Any
from mcp.server import Server, NotificationOptions
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent
from .adapters.mcp_adapter import langchain_tool_to_mcp_tool, execute_langchain_tool
from .registry import discover_tools

logger = logging.getLogger("argentor-lc-bridge")

server = Server("argentor-langchain-bridge")
_tools_cache: dict[str, Any] = {}


@server.list_tools()
async def list_tools() -> list[Tool]:
    """Return all discovered LangChain tools as MCP tools."""
    if not _tools_cache:
        for lc_tool in discover_tools():
            mcp_tool = langchain_tool_to_mcp_tool(lc_tool)
            _tools_cache[mcp_tool.name] = lc_tool
    return [langchain_tool_to_mcp_tool(t) for t in _tools_cache.values()]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[TextContent]:
    """Execute a LangChain tool with the given arguments."""
    if name not in _tools_cache:
        return [TextContent(type="text", text=f"Unknown tool: {name}")]
    
    lc_tool = _tools_cache[name]
    result = await execute_langchain_tool(lc_tool, arguments)
    return [TextContent(type="text", text=str(result))]


async def main():
    """Run the MCP server over stdio."""
    async with stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            server.create_initialization_options(),
        )


if __name__ == "__main__":
    asyncio.run(main())
```

## Step 5 — Adapter

```python
"""Convert between LangChain Tool and MCP Tool formats."""
from mcp.types import Tool
from langchain.tools import BaseTool


def langchain_tool_to_mcp_tool(lc_tool: BaseTool) -> Tool:
    """Convert a LangChain Tool to MCP Tool format."""
    schema = lc_tool.args_schema.schema() if lc_tool.args_schema else {
        "type": "object",
        "properties": {
            "input": {"type": "string", "description": "Tool input"}
        },
        "required": ["input"],
    }
    return Tool(
        name=lc_tool.name,
        description=lc_tool.description or f"LangChain tool: {lc_tool.name}",
        inputSchema=schema,
    )


async def execute_langchain_tool(lc_tool: BaseTool, arguments: dict) -> str:
    """Execute a LangChain tool with given arguments."""
    if hasattr(lc_tool, "ainvoke"):
        result = await lc_tool.ainvoke(arguments)
    else:
        result = lc_tool.invoke(arguments)
    return str(result)
```

## Step 6 — Tool registry

```python
"""Discover available LangChain tools."""
from langchain.tools import BaseTool


def discover_tools() -> list[BaseTool]:
    """Return all discoverable LangChain tools."""
    tools = []
    
    # Try common categories
    try:
        from langchain_community.tools import (
            DuckDuckGoSearchRun,
            WikipediaQueryRun,
            ShellTool,
            PythonREPLTool,
        )
        tools.extend([
            DuckDuckGoSearchRun(),
            # WikipediaQueryRun(),  # needs api wrapper
            # ShellTool(),  # security: opt-in
            # PythonREPLTool(),  # security: opt-in
        ])
    except ImportError:
        pass
    
    return tools


def list_categories() -> dict[str, list[str]]:
    """Return tool categories with available tool class names."""
    return {
        "search": ["DuckDuckGoSearchRun", "GoogleSearchRun", "BraveSearch"],
        "database": ["QuerySQLDataBaseTool", "InfoSQLDatabaseTool"],
        "filesystem": ["ReadFileTool", "WriteFileTool", "ListDirectoryTool"],
        "web": ["RequestsGetTool", "RequestsPostTool"],
        "code": ["PythonREPLTool", "ShellTool"],
    }
```

## Step 7 — README

````markdown
# argentor-langchain-bridge

MCP server that exposes LangChain tools to [Argentor](https://github.com/fboiero/Argentor) and any MCP-compatible client.

## Why?

Argentor is a Rust-based AI agent framework. LangChain has 1,000+ Python integrations.
This bridge lets Argentor agents use any LangChain tool via the standardized MCP protocol —
no Rust rewriting required.

## Install

```bash
pip install argentor-langchain-bridge
```

## Use with Argentor

In `argentor.toml`:

```toml
[mcp.servers.langchain]
command = "argentor-lc-bridge"
description = "LangChain tools via MCP"
```

Then in your agent, all LangChain tools are available as Argentor skills.

## Use with any MCP client

```bash
argentor-lc-bridge
# Speaks MCP over stdio — connect any MCP client
```
````

## Step 8 — Publish to PyPI

```bash
cd argentor-langchain-bridge
pip install -e ".[dev]"
pytest
python -m build
twine upload dist/*
```

## Step 9 — Update Argentor docs

Once published, update `docs/MCP_REGISTRY.md` in the main Argentor repo:

```markdown
### LangChain Bridge
- **Repo**: github.com/fboiero/argentor-langchain-bridge
- **What**: Exposes LangChain's 1,000+ tools as MCP tools
- **Auth**: depends on individual tool
- **Config**:
```toml
[mcp.servers.langchain]
command = "argentor-lc-bridge"
```
```

## Estimated effort

- Day 1: Scaffold, server.py, basic adapter (3-4 tools working)
- Day 2: Expand to 10+ tool categories
- Day 3: CI/CD, tests, publish to PyPI
- Day 4: Documentation, examples
- Total: ~1 week solo, 2-3 days with help

## Maintenance notes

- **LangChain breaking changes**: pin major version in pyproject.toml
- **Tool deprecation**: keep a deprecation list in the README
- **Security**: don't auto-enable shell/REPL tools — make them opt-in via env vars

## Alternative: Don't build it (recommended for now)

Honest assessment: if you don't have time to maintain a Python project, **don't start this**. 
The MCP ecosystem is already producing native MCP servers for most use cases — wait 6 months 
and many LangChain integrations will have native MCP equivalents, making this bridge unnecessary.

Better use of time: focus on Argentor core differentiation (security, intelligence, performance)
and document MCP integration well. The community will build bridges if the demand exists.
