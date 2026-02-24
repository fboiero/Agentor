use agentor_core::AgentorResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TranscriptEvent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptEvent {
    UserMessage {
        content: String,
    },
    AssistantMessage {
        content: String,
    },
    ToolCallRequest {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolCallResult {
        call_id: String,
        tool_name: String,
        content: String,
        is_error: bool,
    },
    SystemEvent {
        event_type: String,
        details: serde_json::Value,
    },
}

// ---------------------------------------------------------------------------
// TranscriptEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub id: Uuid,
    pub session_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub turn: u32,
    pub event: TranscriptEvent,
}

// ---------------------------------------------------------------------------
// TranscriptStore trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait TranscriptStore: Send + Sync {
    async fn append(&self, entry: TranscriptEntry) -> AgentorResult<()>;
    async fn read(&self, session_id: Uuid) -> AgentorResult<Vec<TranscriptEntry>>;
}

// ---------------------------------------------------------------------------
// FileTranscriptStore
// ---------------------------------------------------------------------------

pub struct FileTranscriptStore {
    dir: PathBuf,
}

impl FileTranscriptStore {
    pub async fn new(dir: PathBuf) -> AgentorResult<Self> {
        tokio::fs::create_dir_all(&dir).await?;
        Ok(Self { dir })
    }

    fn transcript_path(&self, session_id: Uuid) -> PathBuf {
        self.dir.join(format!("{}.transcript.jsonl", session_id))
    }
}

#[async_trait]
impl TranscriptStore for FileTranscriptStore {
    async fn append(&self, entry: TranscriptEntry) -> AgentorResult<()> {
        let path = self.transcript_path(entry.session_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        let mut line = serde_json::to_string(&entry)?;
        line.push('\n');
        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    async fn read(&self, session_id: Uuid) -> AgentorResult<Vec<TranscriptEntry>> {
        let path = self.transcript_path(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = tokio::fs::read_to_string(&path).await?;
        let mut entries: Vec<TranscriptEntry> = data
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|a, b| a.turn.cmp(&b.turn).then_with(|| a.timestamp.cmp(&b.timestamp)));
        Ok(entries)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(session_id: Uuid, turn: u32, event: TranscriptEvent) -> TranscriptEntry {
        TranscriptEntry {
            id: Uuid::new_v4(),
            session_id,
            timestamp: Utc::now(),
            turn,
            event,
        }
    }

    #[tokio::test]
    async fn append_and_read_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = FileTranscriptStore::new(tmp.path().to_path_buf())
            .await
            .unwrap();
        let sid = Uuid::new_v4();

        let entry = make_entry(
            sid,
            1,
            TranscriptEvent::UserMessage {
                content: "hello".into(),
            },
        );
        store.append(entry.clone()).await.unwrap();

        let entries = store.read(sid).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
        assert_eq!(entries[0].turn, 1);
        if let TranscriptEvent::UserMessage { content } = &entries[0].event {
            assert_eq!(content, "hello");
        } else {
            panic!("expected UserMessage");
        }
    }

    #[tokio::test]
    async fn empty_transcript_returns_empty_vec() {
        let tmp = TempDir::new().unwrap();
        let store = FileTranscriptStore::new(tmp.path().to_path_buf())
            .await
            .unwrap();
        let sid = Uuid::new_v4();

        let entries = store.read(sid).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn ordering_by_turn() {
        let tmp = TempDir::new().unwrap();
        let store = FileTranscriptStore::new(tmp.path().to_path_buf())
            .await
            .unwrap();
        let sid = Uuid::new_v4();

        // Append out of order: turn 3, then 1, then 2.
        let e3 = make_entry(
            sid,
            3,
            TranscriptEvent::AssistantMessage {
                content: "third".into(),
            },
        );
        let e1 = make_entry(
            sid,
            1,
            TranscriptEvent::UserMessage {
                content: "first".into(),
            },
        );
        let e2 = make_entry(
            sid,
            2,
            TranscriptEvent::AssistantMessage {
                content: "second".into(),
            },
        );
        store.append(e3).await.unwrap();
        store.append(e1).await.unwrap();
        store.append(e2).await.unwrap();

        let entries = store.read(sid).await.unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].turn, 1);
        assert_eq!(entries[1].turn, 2);
        assert_eq!(entries[2].turn, 3);
    }

    #[tokio::test]
    async fn multiple_event_types() {
        let tmp = TempDir::new().unwrap();
        let store = FileTranscriptStore::new(tmp.path().to_path_buf())
            .await
            .unwrap();
        let sid = Uuid::new_v4();

        let events: Vec<TranscriptEvent> = vec![
            TranscriptEvent::UserMessage {
                content: "hi".into(),
            },
            TranscriptEvent::AssistantMessage {
                content: "hello".into(),
            },
            TranscriptEvent::ToolCallRequest {
                call_id: "tc-1".into(),
                tool_name: "echo".into(),
                arguments: serde_json::json!({"text": "ping"}),
            },
            TranscriptEvent::ToolCallResult {
                call_id: "tc-1".into(),
                tool_name: "echo".into(),
                content: "ping".into(),
                is_error: false,
            },
            TranscriptEvent::SystemEvent {
                event_type: "skill_loaded".into(),
                details: serde_json::json!({"name": "echo"}),
            },
        ];

        for (i, ev) in events.into_iter().enumerate() {
            store
                .append(make_entry(sid, i as u32, ev))
                .await
                .unwrap();
        }

        let entries = store.read(sid).await.unwrap();
        assert_eq!(entries.len(), 5);

        // Verify discriminants through pattern matching.
        assert!(matches!(&entries[0].event, TranscriptEvent::UserMessage { .. }));
        assert!(matches!(&entries[1].event, TranscriptEvent::AssistantMessage { .. }));
        assert!(matches!(&entries[2].event, TranscriptEvent::ToolCallRequest { .. }));
        assert!(matches!(&entries[3].event, TranscriptEvent::ToolCallResult { .. }));
        assert!(matches!(&entries[4].event, TranscriptEvent::SystemEvent { .. }));
    }

    #[tokio::test]
    async fn file_persistence_across_store_instances() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();
        let sid = Uuid::new_v4();

        // Write with first store instance.
        {
            let store = FileTranscriptStore::new(dir.clone()).await.unwrap();
            store
                .append(make_entry(
                    sid,
                    1,
                    TranscriptEvent::UserMessage {
                        content: "persist me".into(),
                    },
                ))
                .await
                .unwrap();
        }

        // Read with a brand-new store instance.
        {
            let store2 = FileTranscriptStore::new(dir).await.unwrap();
            let entries = store2.read(sid).await.unwrap();
            assert_eq!(entries.len(), 1);
            if let TranscriptEvent::UserMessage { content } = &entries[0].event {
                assert_eq!(content, "persist me");
            } else {
                panic!("expected UserMessage");
            }
        }
    }
}
