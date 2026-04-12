"""
Example: expose a Python callable (or LangChain tool) to Argentor.

This example shows three patterns:

1. Wrap a plain Python function as an Argentor skill.
2. Wrap a LangChain tool through the LangChainAdapter.
3. Discover all callables in a Python module.

Prerequisites
-------------

    pip install maturin
    cd crates/argentor-python && maturin develop

For the LangChain example:

    pip install langchain langchain-community

License: AGPL-3.0-only
"""

from __future__ import annotations

import json
import math

# --------------------------------------------------------------------------- #
# 1. Expose a plain Python function as an Argentor skill
# --------------------------------------------------------------------------- #


def compute_circle_area(radius: float) -> float:
    """Return the area of a circle. Exposed to Argentor as `circle_area`."""
    return math.pi * radius * radius


def _demo_plain_function() -> None:
    # Once `maturin develop` is run, the module is `argentor` in the same
    # interpreter. The import is inside the function so this file is still
    # syntactically valid without the extension module built.
    import argentor  # type: ignore[import-not-found]

    # The PyO3 bridge loads callables by (module, callable) pair, so the
    # function must live in an importable module. The easiest way for an
    # ad-hoc Python function is to drop it into a file on `sys.path`.
    #
    # Here we assume this file is importable as `load_langchain_tool` (the
    # maturin build places `crates/argentor-python/examples` on sys.path
    # when invoked with `python -m examples.load_langchain_tool`).
    cfg_json = json.dumps(
        {
            "type": "object",
            "properties": {"radius": {"type": "number"}},
            "required": ["radius"],
        }
    )
    # The Rust API is exposed as PythonToolConfig + PythonToolSkill once
    # we add Python bindings for them. Until then, the Rust side is the
    # authoritative caller -- this docstring just shows the intent.
    print("argentor version:", argentor.version())
    print("configured schema:", cfg_json)


# --------------------------------------------------------------------------- #
# 2. Load a LangChain tool
# --------------------------------------------------------------------------- #


def _demo_langchain_tool() -> None:
    """Show how a Python user would prepare a LangChain tool for Argentor.

    Argentor's Rust-side `LangChainAdapter::load_tool(class_name)` tries
    `langchain.tools`, `langchain_community.tools`, and `langchain_core.tools`
    in order. From Python the user's job is simply to install the right
    packages.
    """
    try:
        from langchain_community.tools import DuckDuckGoSearchRun  # type: ignore

        tool = DuckDuckGoSearchRun()
        print("tool.name        =", tool.name)
        print("tool.description =", tool.description[:60], "...")
        # Argentor will serialize the tool spec and expose it to the LLM.
        # The actual call happens inside Rust via PyO3 on demand.
    except ImportError:
        print("langchain_community not installed -- `pip install langchain-community`")


# --------------------------------------------------------------------------- #
# 3. Discover callables in a module
# --------------------------------------------------------------------------- #


def _demo_discovery() -> None:
    """Simulate what `argentor::discover_python_tools("math", None)` does.

    The Rust side introspects the module with `dir()` + `callable()`. This
    Python equivalent is useful for testing the discovery pattern before
    crossing the FFI boundary.
    """
    import math

    public = [n for n in dir(math) if not n.startswith("_") and callable(getattr(math, n))]
    print(f"math module exposes {len(public)} callables, e.g.: {public[:5]}")


# --------------------------------------------------------------------------- #
# Main
# --------------------------------------------------------------------------- #


if __name__ == "__main__":
    print("-- plain function --")
    _demo_plain_function()
    print()
    print("-- LangChain tool --")
    _demo_langchain_tool()
    print()
    print("-- discovery --")
    _demo_discovery()
