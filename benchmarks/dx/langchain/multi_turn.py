"""LangChain — multi-turn conversation with history.

Net LOC: 18
Requires: pip install langchain langchain-anthropic

LangChain uses ChatMessageHistory + RunnableWithMessageHistory
for multi-turn. This is the current idiomatic LCEL pattern.
"""
from langchain_anthropic import ChatAnthropic
from langchain_core.chat_history import InMemoryChatMessageHistory
from langchain_core.runnables.history import RunnableWithMessageHistory
from langchain_core.prompts import ChatPromptTemplate, MessagesPlaceholder

llm = ChatAnthropic(model="claude-sonnet-4-5")

prompt = ChatPromptTemplate.from_messages([
    ("system", "You are a helpful coding assistant."),
    MessagesPlaceholder(variable_name="history"),
    ("human", "{input}"),
])

chain = prompt | llm

store = {}
def get_history(session_id: str) -> InMemoryChatMessageHistory:
    if session_id not in store:
        store[session_id] = InMemoryChatMessageHistory()
    return store[session_id]

runnable = RunnableWithMessageHistory(chain, get_history, input_messages_key="input", history_messages_key="history")

cfg = {"configurable": {"session_id": "s1"}}
r1 = runnable.invoke({"input": "What is a closure in Rust?"}, config=cfg)
print(f"Turn 1: {r1.content}")

r2 = runnable.invoke({"input": "Can you give me a code example?"}, config=cfg)
print(f"Turn 2: {r2.content}")

r3 = runnable.invoke({"input": "How does that differ from Python closures?"}, config=cfg)
print(f"Turn 3: {r3.content}")

# --- LOC count (net, no blanks/comments) ---
# imports: 4
# llm = ...: 1
# prompt = ... (3 lines): 3
# chain = ...: 1
# store = {}: 1
# def get_history (3 lines): 3
# runnable = ...: 1
# cfg = ...: 1
# r1/print + r2/print + r3/print: 6
# TOTAL: 21 net LOC
