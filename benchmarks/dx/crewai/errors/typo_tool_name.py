"""Bug 1: Typo in tool name — 'get_wether' in task description.

CrewAI error quality analysis:
CrewAI tools are referenced by the LLM in natural language.
A typo in the tool name in the task description causes the LLM to either:
  (a) call no tool and hallucinate (no error raised, silent failure), OR
  (b) if the LLM tries to call a non-existent tool: ToolException with
      minimal context — just 'Tool not found'.
Score: file/line — 0 (runtime, LLM-driven)
       names problem — 3 (generic "Tool not found" message)
       suggests fix — 0 (no suggestions, no available-tools list)
Total diagnostic score: 3/30 → 1.0/10

CrewAI's weakest diagnostic area: tool errors are often silent (LLM
hallucinates instead of erroring) or give minimal context when they do fail.
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
    goal="Use the get_wether tool to check weather.",  # BUG: typo in goal
    backstory="You are a weather expert.",
    llm=llm,
    tools=[get_weather],
    verbose=False,
)

task = Task(
    description="Use get_wether to check Paris weather.",  # BUG: typo
    expected_output="Weather report.",
    agent=agent,
)

crew = Crew(agents=[agent], tasks=[task], verbose=False)
result = crew.kickoff()
print(result)
