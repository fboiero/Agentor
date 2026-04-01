//! Pub/sub event bus for decoupled component communication.
//!
//! Provides a lightweight in-process event system that components can
//! use to emit and subscribe to events without direct coupling.
//!
//! # Main types
//!
//! - [`EventBus`] — Central event dispatcher.
//! - [`Event`] — A typed event with topic and payload.
//! - [`Subscription`] — A handle to a topic subscription.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// A discrete event that can be published to the event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event ID.
    pub id: u64,
    /// Topic/channel name.
    pub topic: String,
    /// Event payload (JSON).
    pub payload: serde_json::Value,
    /// When the event was published.
    pub timestamp: DateTime<Utc>,
    /// Optional source identifier.
    pub source: Option<String>,
}

impl Event {
    /// Create a new event.
    pub fn new(topic: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: 0, // Set by EventBus
            topic: topic.into(),
            payload,
            timestamp: Utc::now(),
            source: None,
        }
    }

    /// Set the source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

/// A subscription handle returned when subscribing to a topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    /// Get the numeric ID.
    pub fn id(&self) -> u64 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// EventHandler
// ---------------------------------------------------------------------------

/// A callback that handles events.
type EventHandler = Box<dyn Fn(&Event) + Send + Sync>;

struct Subscriber {
    id: SubscriptionId,
    handler: EventHandler,
}

impl std::fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber").field("id", &self.id).finish()
    }
}

// ---------------------------------------------------------------------------
// EventBusStats
// ---------------------------------------------------------------------------

/// Statistics for the event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusStats {
    /// Total events published.
    pub total_published: u64,
    /// Total events delivered to subscribers.
    pub total_delivered: u64,
    /// Number of active subscriptions.
    pub active_subscriptions: usize,
    /// Number of topics with at least one subscriber.
    pub active_topics: usize,
    /// Events per topic.
    pub events_per_topic: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// Inner state
// ---------------------------------------------------------------------------

struct Inner {
    subscribers: HashMap<String, Vec<Subscriber>>,
    event_history: Vec<Event>,
    max_history: usize,
    topic_counts: HashMap<String, u64>,
    total_published: u64,
    total_delivered: u64,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("topics", &self.subscribers.keys().collect::<Vec<_>>())
            .field("total_published", &self.total_published)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// Central event dispatcher with pub/sub semantics.
///
/// Clone is cheap (inner state is behind `Arc<RwLock>`).
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<RwLock<Inner>>,
    next_event_id: Arc<AtomicU64>,
    next_sub_id: Arc<AtomicU64>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus").finish()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl EventBus {
    /// Create a new event bus with the given history capacity.
    pub fn new(max_history: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                subscribers: HashMap::new(),
                event_history: Vec::new(),
                max_history,
                topic_counts: HashMap::new(),
                total_published: 0,
                total_delivered: 0,
            })),
            next_event_id: Arc::new(AtomicU64::new(1)),
            next_sub_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Subscribe to events on a topic.
    pub fn subscribe(
        &self,
        topic: impl Into<String>,
        handler: impl Fn(&Event) + Send + Sync + 'static,
    ) -> SubscriptionId {
        let id = SubscriptionId(self.next_sub_id.fetch_add(1, Ordering::Relaxed));
        let subscriber = Subscriber {
            id: id.clone(),
            handler: Box::new(handler),
        };

        let topic = topic.into();
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.subscribers.entry(topic).or_default().push(subscriber);

        id
    }

    /// Unsubscribe from events.
    pub fn unsubscribe(&self, id: &SubscriptionId) -> bool {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut found = false;
        for subs in inner.subscribers.values_mut() {
            let before = subs.len();
            subs.retain(|s| s.id != *id);
            if subs.len() < before {
                found = true;
            }
        }
        // Clean up empty topic entries
        inner.subscribers.retain(|_, v| !v.is_empty());
        found
    }

    /// Publish an event to all subscribers on its topic.
    pub fn publish(&self, mut event: Event) -> u64 {
        event.id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
        let event_id = event.id;

        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        inner.total_published += 1;
        *inner.topic_counts.entry(event.topic.clone()).or_insert(0) += 1;

        // Deliver to subscribers
        let mut delivered = 0u64;
        if let Some(subs) = inner.subscribers.get(&event.topic) {
            for sub in subs {
                (sub.handler)(&event);
                delivered += 1;
            }
        }
        inner.total_delivered += delivered;

        // Store in history
        if inner.event_history.len() >= inner.max_history {
            inner.event_history.remove(0);
        }
        inner.event_history.push(event);

        event_id
    }

    /// Publish a simple event with a string payload.
    pub fn emit(&self, topic: impl Into<String>, message: impl Into<String>) -> u64 {
        self.publish(Event::new(topic, serde_json::Value::String(message.into())))
    }

    /// Get recent events for a topic.
    pub fn history(&self, topic: &str, limit: usize) -> Vec<Event> {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner
            .event_history
            .iter()
            .rev()
            .filter(|e| e.topic == topic)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get all recent events.
    pub fn all_history(&self, limit: usize) -> Vec<Event> {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner
            .event_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get event bus statistics.
    pub fn stats(&self) -> EventBusStats {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let active_subscriptions: usize = inner.subscribers.values().map(|s| s.len()).sum();

        EventBusStats {
            total_published: inner.total_published,
            total_delivered: inner.total_delivered,
            active_subscriptions,
            active_topics: inner.subscribers.len(),
            events_per_topic: inner.topic_counts.clone(),
        }
    }

    /// Get the number of subscribers for a topic.
    pub fn subscriber_count(&self, topic: &str) -> usize {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .subscribers
            .get(topic)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Clear all event history.
    pub fn clear_history(&self) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.event_history.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    fn bus() -> EventBus {
        EventBus::new(100)
    }

    // 1. New bus is empty
    #[test]
    fn test_new_bus() {
        let b = bus();
        let stats = b.stats();
        assert_eq!(stats.total_published, 0);
        assert_eq!(stats.active_subscriptions, 0);
    }

    // 2. Subscribe and publish
    #[test]
    fn test_subscribe_publish() {
        let b = bus();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        b.subscribe("test", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        b.emit("test", "hello");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // 3. Multiple subscribers
    #[test]
    fn test_multiple_subscribers() {
        let b = bus();
        let counter = Arc::new(AtomicU32::new(0));

        for _ in 0..3 {
            let c = counter.clone();
            b.subscribe("topic", move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        b.emit("topic", "msg");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    // 4. Topic isolation
    #[test]
    fn test_topic_isolation() {
        let b = bus();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        b.subscribe("topic-a", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        b.emit("topic-b", "msg");
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    // 5. Unsubscribe
    #[test]
    fn test_unsubscribe() {
        let b = bus();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let id = b.subscribe("test", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        b.emit("test", "first");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        assert!(b.unsubscribe(&id));
        b.emit("test", "second");
        assert_eq!(counter.load(Ordering::SeqCst), 1); // no change
    }

    // 6. Unsubscribe nonexistent
    #[test]
    fn test_unsubscribe_nonexistent() {
        let b = bus();
        assert!(!b.unsubscribe(&SubscriptionId(999)));
    }

    // 7. Event history
    #[test]
    fn test_history() {
        let b = bus();
        b.emit("log", "msg1");
        b.emit("log", "msg2");
        b.emit("other", "msg3");

        let history = b.history("log", 10);
        assert_eq!(history.len(), 2);
    }

    // 8. History limit
    #[test]
    fn test_history_limit() {
        let b = bus();
        for i in 0..10 {
            b.emit("log", format!("msg-{i}"));
        }
        let history = b.history("log", 3);
        assert_eq!(history.len(), 3);
    }

    // 9. Event IDs are unique
    #[test]
    fn test_event_ids() {
        let b = bus();
        let id1 = b.emit("test", "a");
        let id2 = b.emit("test", "b");
        assert_ne!(id1, id2);
        assert!(id2 > id1);
    }

    // 10. Stats tracking
    #[test]
    fn test_stats() {
        let b = bus();
        b.subscribe("a", |_| {});
        b.subscribe("a", |_| {});
        b.subscribe("b", |_| {});

        b.emit("a", "x");
        b.emit("b", "y");

        let stats = b.stats();
        assert_eq!(stats.total_published, 2);
        assert_eq!(stats.total_delivered, 3); // 2 on "a" + 1 on "b"
        assert_eq!(stats.active_subscriptions, 3);
        assert_eq!(stats.active_topics, 2);
    }

    // 11. Subscriber count per topic
    #[test]
    fn test_subscriber_count() {
        let b = bus();
        b.subscribe("t", |_| {});
        b.subscribe("t", |_| {});
        assert_eq!(b.subscriber_count("t"), 2);
        assert_eq!(b.subscriber_count("other"), 0);
    }

    // 12. Clear history
    #[test]
    fn test_clear_history() {
        let b = bus();
        b.emit("test", "msg");
        assert!(!b.all_history(10).is_empty());

        b.clear_history();
        assert!(b.all_history(10).is_empty());
    }

    // 13. Event with source
    #[test]
    fn test_event_with_source() {
        let b = bus();
        b.publish(Event::new("test", serde_json::json!("data")).with_source("agent-1"));

        let history = b.history("test", 1);
        assert_eq!(history[0].source.as_deref(), Some("agent-1"));
    }

    // 14. JSON payload
    #[test]
    fn test_json_payload() {
        let b = bus();
        let received = Arc::new(RwLock::new(serde_json::Value::Null));
        let r = received.clone();

        b.subscribe("data", move |e| {
            *r.write().unwrap() = e.payload.clone();
        });

        b.publish(Event::new("data", serde_json::json!({"key": "value"})));

        let val = received.read().unwrap().clone();
        assert_eq!(val["key"], "value");
    }

    // 15. History max capacity
    #[test]
    fn test_history_capacity() {
        let b = EventBus::new(5);
        for i in 0..10 {
            b.emit("test", format!("msg-{i}"));
        }
        assert_eq!(b.all_history(100).len(), 5);
    }

    // 16. Clone shares state
    #[test]
    fn test_clone_shares() {
        let b1 = bus();
        let b2 = b1.clone();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        b1.subscribe("t", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });
        b2.emit("t", "from clone");

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // 17. Default bus
    #[test]
    fn test_default() {
        let b = EventBus::default();
        assert_eq!(b.stats().total_published, 0);
    }

    // 18. Event serializable
    #[test]
    fn test_event_serializable() {
        let mut event = Event::new("topic", serde_json::json!("data"));
        event.id = 42;
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"topic\":\"topic\""));
        let restored: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 42);
    }

    // 19. Stats serializable
    #[test]
    fn test_stats_serializable() {
        let b = bus();
        b.emit("t", "x");
        let stats = b.stats();
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_published\":1"));
    }

    // 20. Events per topic in stats
    #[test]
    fn test_events_per_topic() {
        let b = bus();
        b.emit("a", "x");
        b.emit("a", "y");
        b.emit("b", "z");

        let stats = b.stats();
        assert_eq!(*stats.events_per_topic.get("a").unwrap(), 2);
        assert_eq!(*stats.events_per_topic.get("b").unwrap(), 1);
    }

    // 21. All history returns across topics
    #[test]
    fn test_all_history() {
        let b = bus();
        b.emit("a", "1");
        b.emit("b", "2");
        b.emit("c", "3");

        let all = b.all_history(10);
        assert_eq!(all.len(), 3);
    }
}
