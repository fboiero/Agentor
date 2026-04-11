import { useState, useRef, useEffect, useCallback } from "react";
import { Send, StopCircle, Trash2, Bot, User } from "lucide-react";
import { sendChatMessage, streamChat } from "../api/client";

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: Date;
}

export default function Chat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [useSSE, setUseSSE] = useState(true);
  const [sessionId, setSessionId] = useState<string | undefined>();
  const abortRef = useRef<AbortController | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, scrollToBottom]);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || isStreaming) return;

    setInput("");
    const userMsg: ChatMessage = {
      role: "user",
      content: text,
      timestamp: new Date(),
    };
    setMessages((prev) => [...prev, userMsg]);

    if (useSSE) {
      // SSE streaming mode
      setIsStreaming(true);
      let accumulated = "";

      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: "", timestamp: new Date() },
      ]);

      abortRef.current = streamChat(
        { message: text, session_id: sessionId },
        (data) => {
          try {
            const parsed = JSON.parse(data) as {
              content?: string;
              data?: string;
              session_id?: string;
            };
            const chunk = parsed.content ?? parsed.data ?? "";
            accumulated += chunk;

            if (parsed.session_id && !sessionId) {
              setSessionId(parsed.session_id);
            }

            setMessages((prev) => {
              const copy = [...prev];
              const last = copy[copy.length - 1];
              if (last?.role === "assistant") {
                copy[copy.length - 1] = { ...last, content: accumulated };
              }
              return copy;
            });
          } catch {
            // Raw text chunk
            accumulated += data;
            setMessages((prev) => {
              const copy = [...prev];
              const last = copy[copy.length - 1];
              if (last?.role === "assistant") {
                copy[copy.length - 1] = { ...last, content: accumulated };
              }
              return copy;
            });
          }
        },
        (err) => {
          setMessages((prev) => [
            ...prev,
            {
              role: "system",
              content: `Error: ${err.message}`,
              timestamp: new Date(),
            },
          ]);
          setIsStreaming(false);
        },
        () => {
          setIsStreaming(false);
        },
      );
    } else {
      // Non-streaming POST /api/v1/agent/chat
      setIsStreaming(true);
      try {
        const resp = await sendChatMessage({
          message: text,
          session_id: sessionId,
        });
        if (resp.session_id) setSessionId(resp.session_id);
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            content: resp.response,
            timestamp: new Date(),
          },
        ]);
      } catch (err) {
        setMessages((prev) => [
          ...prev,
          {
            role: "system",
            content: `Error: ${err instanceof Error ? err.message : String(err)}`,
            timestamp: new Date(),
          },
        ]);
      }
      setIsStreaming(false);
    }
  };

  const handleStop = () => {
    abortRef.current?.abort();
    setIsStreaming(false);
  };

  const handleClear = () => {
    setMessages([]);
    setSessionId(undefined);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  };

  return (
    <section className="chat-page">
      <div className="section-header">
        <div>
          <h1 className="section-title">Chat</h1>
          <p className="section-subtitle">
            Interact with agents via chat
            {sessionId && (
              <span className="chat-session-id"> | Session: {sessionId.slice(0, 8)}...</span>
            )}
          </p>
        </div>
        <div className="btn-group">
          <label className="chat-toggle">
            <input
              type="checkbox"
              checked={useSSE}
              onChange={(e) => setUseSSE(e.target.checked)}
            />
            <span>Stream (SSE)</span>
          </label>
          <button
            className="btn btn--sm"
            onClick={handleClear}
            disabled={isStreaming}
          >
            <Trash2 size={14} /> Clear
          </button>
        </div>
      </div>

      {/* Messages */}
      <div className="chat-messages">
        {messages.length === 0 && (
          <div className="chat-empty">
            <Bot size={48} strokeWidth={1.2} />
            <p>Send a message to start chatting with the agent.</p>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`chat-message chat-message--${msg.role}`}>
            <div className="chat-message__avatar">
              {msg.role === "user" ? (
                <User size={18} />
              ) : (
                <Bot size={18} />
              )}
            </div>
            <div className="chat-message__content">
              <div className="chat-message__text">
                {msg.content || (
                  <span className="chat-typing">
                    <span />
                    <span />
                    <span />
                  </span>
                )}
              </div>
              <div className="chat-message__time">
                {msg.timestamp.toLocaleTimeString("en-US", { hour12: false })}
              </div>
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="chat-input-bar">
        <textarea
          className="chat-input"
          placeholder="Type a message... (Enter to send, Shift+Enter for newline)"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          rows={1}
          disabled={isStreaming}
        />
        {isStreaming ? (
          <button className="btn btn--danger btn--icon" onClick={handleStop}>
            <StopCircle size={18} />
          </button>
        ) : (
          <button
            className="btn btn--primary btn--icon"
            onClick={() => void handleSend()}
            disabled={!input.trim()}
          >
            <Send size={18} />
          </button>
        )}
      </div>
    </section>
  );
}
