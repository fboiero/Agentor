use agentor_core::Message;
use agentor_session::{FileSessionStore, Session, SessionStore};
use uuid::Uuid;

/// Helper: create a FileSessionStore in a temp directory.
async fn temp_store() -> (FileSessionStore, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let store = FileSessionStore::new(tmp.path().join("sessions"))
        .await
        .unwrap();
    (store, tmp)
}

#[tokio::test]
async fn test_create_and_get_session() {
    let (store, _tmp) = temp_store().await;
    let session = Session::new();
    let id = session.id;

    store.create(&session).await.unwrap();

    let loaded = store.get(id).await.unwrap().unwrap();
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.messages.len(), 0);
}

#[tokio::test]
async fn test_get_nonexistent_returns_none() {
    let (store, _tmp) = temp_store().await;
    let result = store.get(Uuid::new_v4()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_create_and_update_session() {
    let (store, _tmp) = temp_store().await;
    let mut session = Session::new();
    let id = session.id;

    store.create(&session).await.unwrap();

    // Add a message and update
    session.add_message(Message::user("Hello!", id));
    store.update(&session).await.unwrap();

    let loaded = store.get(id).await.unwrap().unwrap();
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].content, "Hello!");
}

#[tokio::test]
async fn test_delete_session() {
    let (store, _tmp) = temp_store().await;
    let session = Session::new();
    let id = session.id;

    store.create(&session).await.unwrap();
    assert!(store.get(id).await.unwrap().is_some());

    store.delete(id).await.unwrap();
    assert!(store.get(id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_is_ok() {
    let (store, _tmp) = temp_store().await;
    // Deleting a session that doesn't exist should not error
    store.delete(Uuid::new_v4()).await.unwrap();
}

#[tokio::test]
async fn test_list_sessions() {
    let (store, _tmp) = temp_store().await;

    let s1 = Session::new();
    let s2 = Session::new();
    let s3 = Session::new();

    store.create(&s1).await.unwrap();
    store.create(&s2).await.unwrap();
    store.create(&s3).await.unwrap();

    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&s1.id));
    assert!(ids.contains(&s2.id));
    assert!(ids.contains(&s3.id));
}

#[tokio::test]
async fn test_list_empty() {
    let (store, _tmp) = temp_store().await;
    let ids = store.list().await.unwrap();
    assert!(ids.is_empty());
}

#[tokio::test]
async fn test_session_preserves_multiple_messages() {
    let (store, _tmp) = temp_store().await;
    let mut session = Session::new();
    let id = session.id;

    session.add_message(Message::user("Question 1", id));
    session.add_message(Message::assistant("Answer 1", id));
    session.add_message(Message::user("Question 2", id));
    session.add_message(Message::assistant("Answer 2", id));

    store.create(&session).await.unwrap();

    let loaded = store.get(id).await.unwrap().unwrap();
    assert_eq!(loaded.messages.len(), 4);
    assert_eq!(loaded.messages[0].content, "Question 1");
    assert_eq!(loaded.messages[1].content, "Answer 1");
    assert_eq!(loaded.messages[2].content, "Question 2");
    assert_eq!(loaded.messages[3].content, "Answer 2");
}

#[tokio::test]
async fn test_session_metadata_persists() {
    let (store, _tmp) = temp_store().await;
    let mut session = Session::new();
    let id = session.id;

    session
        .metadata
        .insert("channel".to_string(), serde_json::json!("telegram"));
    session
        .metadata
        .insert("user_id".to_string(), serde_json::json!(12345));

    store.create(&session).await.unwrap();

    let loaded = store.get(id).await.unwrap().unwrap();
    assert_eq!(loaded.metadata["channel"], "telegram");
    assert_eq!(loaded.metadata["user_id"], 12345);
}

#[tokio::test]
async fn test_session_active_skills_persist() {
    let (store, _tmp) = temp_store().await;
    let mut session = Session::new();
    let id = session.id;

    session.active_skills.push("shell".to_string());
    session.active_skills.push("http_fetch".to_string());

    store.create(&session).await.unwrap();

    let loaded = store.get(id).await.unwrap().unwrap();
    assert_eq!(loaded.active_skills.len(), 2);
    assert!(loaded.active_skills.contains(&"shell".to_string()));
    assert!(loaded.active_skills.contains(&"http_fetch".to_string()));
}

#[tokio::test]
async fn test_create_after_delete_works() {
    let (store, _tmp) = temp_store().await;
    let session = Session::new();
    let id = session.id;

    store.create(&session).await.unwrap();
    store.delete(id).await.unwrap();
    assert!(store.get(id).await.unwrap().is_none());

    // Re-create with same ID
    store.create(&session).await.unwrap();
    assert!(store.get(id).await.unwrap().is_some());
}
