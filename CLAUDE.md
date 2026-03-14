# Claude Project Configuration

## Working Style
- Never ask yes/no confirmation questions mid-task.
- Once a plan is agreed upon, execute it completely without interrupting.
- Only stop if you hit a hard blocker that makes it impossible to continue.
- If ambiguous, make the most reasonable decision and explain it in the final summary.

## Context Management
- At session start: check if CONTEXT.md exists, read it, and resume from there.
- When asked to "save context" or at end of a significant task, update CONTEXT.md with:
  - Current goal and plan
  - What's completed
  - What's pending
  - Key decisions made
  - Relevant file paths or commands

## Decision & Prompt Log
- After every significant decision or completed task, append an entry to DECISIONS.md with:
  - Timestamp
  - What was asked (summarized)
  - What decision was made and why
  - Alternatives considered (if any)
  - Estimated token usage (input/output if available)
  - Notes on what made the prompt effective or ineffective
