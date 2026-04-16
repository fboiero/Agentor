"""LangChain — agent with one tool (get_weather).

Net LOC: 22
Requires: pip install langchain langchain-anthropic

LangChain uses @tool decorator for function-based tool definition.
AgentExecutor wraps the agent + tool list.
"""
from langchain_anthropic import ChatAnthropic
from langchain.agents import AgentExecutor, create_tool_calling_agent
from langchain_core.prompts import ChatPromptTemplate
from langchain_core.tools import tool

@tool
def get_weather(city: str) -> str:
    """Return current weather for a city."""
    return f"Weather in {city}: sunny, 22°C"

llm = ChatAnthropic(model="claude-sonnet-4-5")

prompt = ChatPromptTemplate.from_messages([
    ("system", "You are a weather assistant."),
    ("placeholder", "{chat_history}"),
    ("human", "{input}"),
    ("placeholder", "{agent_scratchpad}"),
])

agent = create_tool_calling_agent(llm, [get_weather], prompt)
executor = AgentExecutor(agent=agent, tools=[get_weather])

result = executor.invoke({"input": "What's the weather in Paris?"})
print(result["output"])

# --- LOC count (net, no blanks/comments) ---
# imports: 4
# @tool + def get_weather + return: 3
# llm = ...: 1
# prompt = ... (4 lines): 4
# agent = ...: 1
# executor = ...: 1
# result = ...: 1
# print: 1
# TOTAL: 16 net LOC
