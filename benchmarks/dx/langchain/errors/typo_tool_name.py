"""Bug 1: Typo in tool name — calling 'get_wether' (typo) instead of 'get_weather'.

LangChain error quality analysis:
When using create_tool_calling_agent, the LLM decides which tool to call.
If the LLM hallucinates 'get_wether', AgentExecutor raises:
  ValueError: Tool 'get_wether' not found in tools list.
  Available tools: ['get_weather']
Score: file/line — 0 (runtime ValueError, no traceback line points to user code)
       names problem — 9 (exact tool name in error + available list)
       suggests fix — 7 (list of available tools implies spelling)
Total diagnostic score: 16/30 → 5.3/10

Note: In static tool-list scenarios (direct ToolNode usage), LangChain
raises during graph construction — better diagnostics. In AgentExecutor
the error is runtime and depends on LLM output.
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
    ("system", "Use the get_wether tool."),  # BUG: typo in system prompt
    ("human", "{input}"),
    ("placeholder", "{agent_scratchpad}"),
])

agent = create_tool_calling_agent(llm, [get_weather], prompt)
executor = AgentExecutor(agent=agent, tools=[get_weather])

# LLM may call 'get_wether' (as instructed in system prompt) → ToolException
result = executor.invoke({"input": "What's the weather in Paris?"})
print(result)
