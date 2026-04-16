"""Bug 3: Malformed prompt template — invalid variable syntax.

LangChain error quality analysis:
LangChain's ChatPromptTemplate uses {variable} syntax (single braces).
A typo like {name (unclosed) raises at template construction:
  ValueError: Single '{' or '}' encountered in format string
  (Raised in PromptTemplate._format — points to internal code, not user code)
Score: file/line — 3 (Python traceback shows user's from_messages call but
                      error originates inside LangChain internals)
       names problem — 6 (mentions format string issue, not which variable)
       suggests fix — 3 (implies you need balanced braces, but unclear how)
Total diagnostic score: 12/30 → 4.0/10

Note: LangChain DOES catch this at construction, which is better than
frameworks that silently pass malformed templates to the LLM.
"""
from langchain_core.prompts import ChatPromptTemplate

# BUG: unclosed brace in template variable
# LangChain uses {var} syntax — single brace.
# A missing closing brace triggers ValueError at construction time.
prompt = ChatPromptTemplate.from_messages([
    ("system", "You are an assistant for {name. Answer their question."),  # BUG
    ("human", "{input}"),
])

print(prompt)
