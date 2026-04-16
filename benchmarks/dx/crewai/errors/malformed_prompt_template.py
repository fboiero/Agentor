"""Bug 3: Malformed prompt template — invalid placeholder in backstory.

CrewAI error quality analysis:
CrewAI uses Python's str.format() for its internal prompt assembly.
A malformed placeholder like {name (missing closing brace) raises:
  ValueError: Single '{' or '}' encountered in format string
  (raised inside crewai/agent.py prompt assembly, not at user code line)
Score: file/line — 2 (traceback reaches crewai internals, not user's line)
       names problem — 5 (format string error is somewhat descriptive)
       suggests fix — 2 (implies balanced braces needed, no example)
Total diagnostic score: 9/30 → 3.0/10

Note: CrewAI also silently ignores some template issues if the field
isn't used in the final assembled prompt — inconsistent behavior.
"""
from crewai import Agent, Task, Crew, LLM

llm = LLM(model="anthropic/claude-sonnet-4-5")

# BUG: malformed template placeholder in backstory
agent = Agent(
    role="Assistant",
    goal="Help users.",
    backstory="You assist {name. They need help.",  # BUG: unclosed {name
    llm=llm,
    verbose=False,
)

task = Task(description="Hello.", expected_output="A response.", agent=agent)
crew = Crew(agents=[agent], tasks=[task], verbose=False)
result = crew.kickoff()  # Error may fire here during prompt assembly
print(result)
