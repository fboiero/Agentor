"""Bug 2: Missing API key — ANTHROPIC_API_KEY not set.

CrewAI error quality analysis:
CrewAI wraps the LLM call, and the error propagates from litellm:
  litellm.exceptions.AuthenticationError: AuthenticationError:
    AnthropicException - {'type': 'error', 'error':
    {'type': 'authentication_error', 'message': 'invalid x-api-key'}}
Score: file/line — 0 (deep stack trace through litellm internals)
       names problem — 4 (mentions api-key but not env var name)
       suggests fix — 0 (no guidance on setting ANTHROPIC_API_KEY)
Total diagnostic score: 4/30 → 1.3/10

Same quality as LangChain — both delegate to underlying SDK/litellm
and don't add their own key-validation layer.
"""
from crewai import Agent, Task, Crew, LLM
import os

# BUG: unset API key
os.environ.pop("ANTHROPIC_API_KEY", None)

llm = LLM(model="anthropic/claude-sonnet-4-5")  # No error here

agent = Agent(
    role="Assistant",
    goal="Answer questions.",
    backstory="You are helpful.",
    llm=llm,
    verbose=False,
)

task = Task(description="Say hello.", expected_output="A greeting.", agent=agent)
crew = Crew(agents=[agent], tasks=[task], verbose=False)
result = crew.kickoff()  # Error fires here
print(result)
