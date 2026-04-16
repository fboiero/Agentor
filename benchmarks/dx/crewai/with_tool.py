"""CrewAI — agent with one tool (get_weather).

Net LOC: 24
Requires: pip install crewai crewai-tools

CrewAI uses @tool decorator (same as LangChain under the hood).
Tool assignment is per-agent via the `tools` parameter.
"""
from crewai import Agent, Task, Crew, LLM
from crewai.tools import tool

@tool("get_weather")
def get_weather(city: str) -> str:
    """Return current weather for a city."""
    return f"Weather in {city}: sunny, 22°C"

llm = LLM(model="anthropic/claude-sonnet-4-5")

agent = Agent(
    role="Weather Assistant",
    goal="Provide accurate weather information.",
    backstory="You are a weather expert with access to real-time data.",
    llm=llm,
    tools=[get_weather],
    verbose=False,
)

task = Task(
    description="What's the weather in Paris?",
    expected_output="Current weather conditions for Paris.",
    agent=agent,
)

crew = Crew(agents=[agent], tasks=[task], verbose=False)
result = crew.kickoff()
print(result)

# --- LOC count (net, no blanks/comments) ---
# imports: 2
# @tool + def + return: 3
# llm = ...: 1
# agent = ... (6 lines): 6
# task = ... (4 lines): 4
# crew = ...; result = ...; print: 3
# TOTAL: 19 net LOC
