"""LangChain — minimal Hello World agent.

Net LOC: 10
Requires: pip install langchain langchain-anthropic
NOTE: Requires ANTHROPIC_API_KEY in environment.

LangChain v0.3 with LCEL (LangChain Expression Language) is the
current idiomatic path. create_react_agent adds ~180-220 tok overhead.
"""
from langchain_anthropic import ChatAnthropic
from langchain_core.messages import HumanMessage

llm = ChatAnthropic(model="claude-sonnet-4-5")

response = llm.invoke([HumanMessage(content="Hello, what can you do?")])

print(response.content)

# --- LOC count (net, no blanks/comments) ---
# from langchain_anthropic import ChatAnthropic          1
# from langchain_core.messages import HumanMessage       1
# llm = ChatAnthropic(model="claude-sonnet-4-5")         1
# response = llm.invoke([HumanMessage(...)])              1
# print(response.content)                                1
# TOTAL: 5 net LOC
#
# Honest note: this is the SHORTEST path — direct LLM call, not an "agent".
# An actual LangChain agent (with tools/AgentExecutor) adds 5-10 more lines.
# We count the simplest working path for TTFA fairness.
