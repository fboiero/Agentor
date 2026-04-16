"""CrewAI — minimal Hello World agent.

Net LOC: 14
Requires: pip install crewai crewai-tools
NOTE: Requires ANTHROPIC_API_KEY or OPENAI_API_KEY in environment.

CrewAI is role-based: every agent has a role, goal, and backstory.
This is the minimum viable single-agent/single-task crew.
The role/goal/backstory triple adds ~500 tok overhead per LLM call.
"""
from crewai import Agent, Task, Crew, LLM

llm = LLM(model="anthropic/claude-sonnet-4-5")

agent = Agent(
    role="General Assistant",
    goal="Answer questions helpfully and concisely.",
    backstory="You are a knowledgeable AI assistant.",
    llm=llm,
    verbose=False,
)

task = Task(
    description="Hello, what can you do?",
    expected_output="A description of capabilities.",
    agent=agent,
)

crew = Crew(agents=[agent], tasks=[task], verbose=False)
result = crew.kickoff()
print(result)

# --- LOC count (net, no blanks/comments) ---
# import: 1
# llm = ...: 1
# agent = ... (5 lines): 5
# task = ... (4 lines): 4
# crew = ...: 1
# result = ...; print: 2
# TOTAL: 14 net LOC
