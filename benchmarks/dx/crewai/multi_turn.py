"""CrewAI — multi-turn conversation with history.

Net LOC: 32
Requires: pip install crewai

IMPORTANT CAVEAT: CrewAI is task-orchestration-first, NOT conversation-first.
Multi-turn conversation requires using the underlying LLM directly with
a Memory object, or chaining multiple Tasks. The most honest approach
is to use CrewAI's built-in memory feature with consecutive tasks.

This is NOT idiomatic CrewAI use — it's a workaround to match the
other frameworks' multi-turn Q&A scenario. CrewAI is designed for
crew-level collaboration (task pipelines), not back-and-forth dialog.
"""
from crewai import Agent, Task, Crew, LLM
from crewai.memory import ConversationMemory

llm = LLM(model="anthropic/claude-sonnet-4-5")

# CrewAI supports memory but it's crew-level, not conversation-level
agent = Agent(
    role="Coding Assistant",
    goal="Help developers understand programming concepts.",
    backstory="You are an expert programmer with deep knowledge of multiple languages.",
    llm=llm,
    memory=True,  # enables short-term memory
    verbose=False,
)

# Multi-turn in CrewAI: define sequential tasks — NOT the same as a
# back-and-forth conversation. The agent sees task descriptions, not a dialog.
task1 = Task(description="What is a closure in Rust?", expected_output="Explanation of closures.", agent=agent)
task2 = Task(description="Give a code example of a Rust closure.", expected_output="Code example.", agent=agent)
task3 = Task(description="How do Rust closures differ from Python closures?", expected_output="Comparison.", agent=agent)

crew = Crew(agents=[agent], tasks=[task1, task2, task3], verbose=False, memory=True)
result = crew.kickoff()
print(result)

# --- LOC count (net, no blanks/comments) ---
# imports: 2
# llm = ...: 1
# agent = ... (7 lines): 7
# task1/task2/task3: 3
# crew = ...; result = ...; print: 3
# TOTAL: 16 net LOC (not counting mandatory workaround comments)
#
# NOTE: This is materially less ergonomic for the multi-turn use case
# than all other frameworks tested. CrewAI is not designed for this.
