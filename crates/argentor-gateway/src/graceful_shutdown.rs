//! Graceful shutdown manager with cleanup hooks and connection draining.
//!
//! Provides a structured way to register shutdown callbacks, drain active
//! connections, and ensure all cleanup runs within a configurable timeout.
//!
//! # Main types
//!
//! - [`ShutdownManager`] — Coordinates graceful shutdown with hooks.
//! - [`ShutdownHook`] — A named cleanup callback.
//! - [`ShutdownPhase`] — Ordered execution phases for hooks.
//! - [`ShutdownReport`] — Summary of what happened during shutdown.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// ShutdownPhase
// ---------------------------------------------------------------------------

/// Ordered phases during graceful shutdown.
///
/// Hooks execute in phase order: `PreDrain` → `Drain` → `Cleanup` → `Final`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownPhase {
    /// Before draining — stop accepting new connections/requests.
    PreDrain,
    /// Drain active connections and in-flight requests.
    Drain,
    /// Clean up resources (flush buffers, close files, save state).
    Cleanup,
    /// Final actions (audit log entries, metric export).
    Final,
}

// ---------------------------------------------------------------------------
// ShutdownHook
// ---------------------------------------------------------------------------

/// A named cleanup callback to run during shutdown.
pub struct ShutdownHook {
    /// Human-readable name for logging.
    pub name: String,
    /// Which phase this hook should run in.
    pub phase: ShutdownPhase,
    /// The callback to execute.
    pub callback: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
}

impl std::fmt::Debug for ShutdownHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShutdownHook")
            .field("name", &self.name)
            .field("phase", &self.phase)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// HookResult
// ---------------------------------------------------------------------------

/// Result of executing a single shutdown hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Name of the hook.
    pub name: String,
    /// Phase the hook ran in.
    pub phase: ShutdownPhase,
    /// Whether the hook succeeded.
    pub success: bool,
    /// Error message if the hook failed.
    pub error: Option<String>,
    /// How long the hook took to run.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// ShutdownReport
// ---------------------------------------------------------------------------

/// Summary of a graceful shutdown sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownReport {
    /// Total wall-clock time for the shutdown sequence.
    pub total_duration_ms: u64,
    /// Number of hooks that ran successfully.
    pub hooks_succeeded: usize,
    /// Number of hooks that failed.
    pub hooks_failed: usize,
    /// Whether the shutdown completed within the timeout.
    pub completed_in_time: bool,
    /// Individual hook results.
    pub hook_results: Vec<HookResult>,
}

// ---------------------------------------------------------------------------
// ShutdownState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShutdownState {
    Running,
    ShuttingDown,
    Completed,
}

// ---------------------------------------------------------------------------
// ShutdownManager
// ---------------------------------------------------------------------------

/// Coordinates graceful shutdown with ordered phases and cleanup hooks.
///
/// Clone is cheap (inner state is behind `Arc`).
#[derive(Clone)]
pub struct ShutdownManager {
    state: Arc<RwLock<ShutdownState>>,
    hooks: Arc<RwLock<Vec<ShutdownHook>>>,
    notify: Arc<Notify>,
    timeout: Duration,
}

impl std::fmt::Debug for ShutdownManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShutdownManager")
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ShutdownManager {
    /// Create a new shutdown manager with the given timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(ShutdownState::Running)),
            hooks: Arc::new(RwLock::new(Vec::new())),
            notify: Arc::new(Notify::new()),
            timeout,
        }
    }

    /// Register a shutdown hook.
    pub async fn register_hook(&self, hook: ShutdownHook) {
        let mut hooks = self.hooks.write().await;
        info!(name = %hook.name, phase = ?hook.phase, "Shutdown hook registered");
        hooks.push(hook);
    }

    /// Register a simple shutdown hook with a name, phase, and closure.
    pub async fn on_shutdown(
        &self,
        name: impl Into<String>,
        phase: ShutdownPhase,
        callback: impl Fn() -> Result<(), String> + Send + Sync + 'static,
    ) {
        self.register_hook(ShutdownHook {
            name: name.into(),
            phase,
            callback: Box::new(callback),
        })
        .await;
    }

    /// Check if shutdown has been initiated.
    pub async fn is_shutting_down(&self) -> bool {
        let state = self.state.read().await;
        *state != ShutdownState::Running
    }

    /// Get a future that resolves when shutdown is initiated.
    ///
    /// This can be used as a signal in `tokio::select!` loops.
    pub fn shutdown_signal(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// Initiate graceful shutdown, running all hooks in phase order.
    ///
    /// Returns a report of what happened.
    pub async fn shutdown(&self) -> ShutdownReport {
        let start = Instant::now();

        // Mark as shutting down
        {
            let mut state = self.state.write().await;
            if *state != ShutdownState::Running {
                return ShutdownReport {
                    total_duration_ms: 0,
                    hooks_succeeded: 0,
                    hooks_failed: 0,
                    completed_in_time: true,
                    hook_results: Vec::new(),
                };
            }
            *state = ShutdownState::ShuttingDown;
        }

        // Notify waiting tasks
        self.notify.notify_waiters();
        info!("Graceful shutdown initiated");

        let mut all_results = Vec::new();
        let mut succeeded = 0;
        let mut failed = 0;

        // Execute hooks in phase order
        let phases = [
            ShutdownPhase::PreDrain,
            ShutdownPhase::Drain,
            ShutdownPhase::Cleanup,
            ShutdownPhase::Final,
        ];

        let hooks = self.hooks.read().await;

        for phase in &phases {
            if start.elapsed() > self.timeout {
                warn!(phase = ?phase, "Shutdown timeout reached, skipping remaining hooks");
                break;
            }

            let phase_hooks: Vec<&ShutdownHook> =
                hooks.iter().filter(|h| h.phase == *phase).collect();

            if !phase_hooks.is_empty() {
                info!(phase = ?phase, count = phase_hooks.len(), "Executing shutdown phase");
            }

            for hook in phase_hooks {
                let hook_start = Instant::now();
                let result = (hook.callback)();
                let duration_ms = hook_start.elapsed().as_millis() as u64;

                match result {
                    Ok(()) => {
                        info!(name = %hook.name, duration_ms, "Shutdown hook completed");
                        succeeded += 1;
                        all_results.push(HookResult {
                            name: hook.name.clone(),
                            phase: *phase,
                            success: true,
                            error: None,
                            duration_ms,
                        });
                    }
                    Err(e) => {
                        error!(name = %hook.name, error = %e, "Shutdown hook failed");
                        failed += 1;
                        all_results.push(HookResult {
                            name: hook.name.clone(),
                            phase: *phase,
                            success: false,
                            error: Some(e),
                            duration_ms,
                        });
                    }
                }
            }
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;
        let completed_in_time = start.elapsed() <= self.timeout;

        // Mark as completed
        {
            let mut state = self.state.write().await;
            *state = ShutdownState::Completed;
        }

        info!(
            total_ms = total_duration_ms,
            succeeded, failed, "Graceful shutdown complete"
        );

        ShutdownReport {
            total_duration_ms,
            hooks_succeeded: succeeded,
            hooks_failed: failed,
            completed_in_time,
            hook_results: all_results,
        }
    }

    /// Get the number of registered hooks.
    pub async fn hook_count(&self) -> usize {
        self.hooks.read().await.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    fn manager() -> ShutdownManager {
        ShutdownManager::new(Duration::from_secs(5))
    }

    // 1. New manager is in running state
    #[tokio::test]
    async fn test_initial_state() {
        let mgr = manager();
        assert!(!mgr.is_shutting_down().await);
    }

    // 2. Shutdown changes state
    #[tokio::test]
    async fn test_shutdown_changes_state() {
        let mgr = manager();
        let _report = mgr.shutdown().await;
        assert!(mgr.is_shutting_down().await);
    }

    // 3. Hooks run on shutdown
    #[tokio::test]
    async fn test_hooks_run() {
        let mgr = manager();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_clone = ran.clone();

        mgr.on_shutdown("test-hook", ShutdownPhase::Cleanup, move || {
            ran_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        let report = mgr.shutdown().await;
        assert!(ran.load(Ordering::SeqCst));
        assert_eq!(report.hooks_succeeded, 1);
        assert_eq!(report.hooks_failed, 0);
    }

    // 4. Failed hooks are tracked
    #[tokio::test]
    async fn test_failed_hooks() {
        let mgr = manager();
        mgr.on_shutdown("fail-hook", ShutdownPhase::Cleanup, || {
            Err("intentional failure".to_string())
        })
        .await;

        let report = mgr.shutdown().await;
        assert_eq!(report.hooks_failed, 1);
        assert_eq!(report.hooks_succeeded, 0);
        assert!(report.hook_results[0].error.is_some());
    }

    // 5. Hooks execute in phase order
    #[tokio::test]
    async fn test_phase_order() {
        let mgr = manager();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        for (name, phase) in [
            ("final", ShutdownPhase::Final),
            ("pre-drain", ShutdownPhase::PreDrain),
            ("cleanup", ShutdownPhase::Cleanup),
            ("drain", ShutdownPhase::Drain),
        ] {
            let order_clone = order.clone();
            let name_str = name.to_string();
            mgr.on_shutdown(name, phase, move || {
                order_clone.lock().unwrap().push(name_str.clone());
                Ok(())
            })
            .await;
        }

        mgr.shutdown().await;

        let order = order.lock().unwrap();
        assert_eq!(&*order, &["pre-drain", "drain", "cleanup", "final"]);
    }

    // 6. Multiple hooks per phase
    #[tokio::test]
    async fn test_multiple_hooks_per_phase() {
        let mgr = manager();
        let counter = Arc::new(AtomicU32::new(0));

        for i in 0..3 {
            let c = counter.clone();
            mgr.on_shutdown(format!("hook-{i}"), ShutdownPhase::Cleanup, move || {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await;
        }

        let report = mgr.shutdown().await;
        assert_eq!(counter.load(Ordering::SeqCst), 3);
        assert_eq!(report.hooks_succeeded, 3);
    }

    // 7. Double shutdown is safe (no-op)
    #[tokio::test]
    async fn test_double_shutdown() {
        let mgr = manager();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        mgr.on_shutdown("once", ShutdownPhase::Cleanup, move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await;

        mgr.shutdown().await;
        let report2 = mgr.shutdown().await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(report2.hooks_succeeded, 0);
    }

    // 8. Report includes timing
    #[tokio::test]
    async fn test_report_timing() {
        let mgr = manager();
        let report = mgr.shutdown().await;
        assert!(report.completed_in_time);
        // Duration should be very small for empty shutdown
        assert!(report.total_duration_ms < 1000);
    }

    // 9. Hook count tracking
    #[tokio::test]
    async fn test_hook_count() {
        let mgr = manager();
        assert_eq!(mgr.hook_count().await, 0);

        mgr.on_shutdown("h1", ShutdownPhase::Cleanup, || Ok(()))
            .await;
        mgr.on_shutdown("h2", ShutdownPhase::Final, || Ok(())).await;
        assert_eq!(mgr.hook_count().await, 2);
    }

    // 10. Shutdown signal notifies waiters
    #[tokio::test]
    async fn test_shutdown_signal() {
        let mgr = manager();
        let signal = mgr.shutdown_signal();
        let notified = Arc::new(AtomicBool::new(false));
        let notified_clone = notified.clone();

        let handle = tokio::spawn(async move {
            signal.notified().await;
            notified_clone.store(true, Ordering::SeqCst);
        });

        // Small delay to ensure the task is waiting
        tokio::time::sleep(Duration::from_millis(10)).await;
        mgr.shutdown().await;

        handle.await.unwrap();
        assert!(notified.load(Ordering::SeqCst));
    }

    // 11. Report serializable
    #[tokio::test]
    async fn test_report_serializable() {
        let mgr = manager();
        mgr.on_shutdown("ser", ShutdownPhase::Cleanup, || Ok(()))
            .await;
        let report = mgr.shutdown().await;

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"hooks_succeeded\":1"));

        let restored: ShutdownReport = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.hooks_succeeded, 1);
    }

    // 12. ShutdownPhase ordering
    #[test]
    fn test_phase_ordering() {
        assert!(ShutdownPhase::PreDrain < ShutdownPhase::Drain);
        assert!(ShutdownPhase::Drain < ShutdownPhase::Cleanup);
        assert!(ShutdownPhase::Cleanup < ShutdownPhase::Final);
    }

    // 13. Hook result tracks duration
    #[tokio::test]
    async fn test_hook_duration() {
        let mgr = manager();
        mgr.on_shutdown("slow", ShutdownPhase::Cleanup, || {
            std::thread::sleep(Duration::from_millis(10));
            Ok(())
        })
        .await;

        let report = mgr.shutdown().await;
        assert!(report.hook_results[0].duration_ms >= 5);
    }

    // 14. Mixed success and failure
    #[tokio::test]
    async fn test_mixed_results() {
        let mgr = manager();
        mgr.on_shutdown("ok1", ShutdownPhase::PreDrain, || Ok(()))
            .await;
        mgr.on_shutdown("fail1", ShutdownPhase::Drain, || Err("oops".to_string()))
            .await;
        mgr.on_shutdown("ok2", ShutdownPhase::Cleanup, || Ok(()))
            .await;
        mgr.on_shutdown("fail2", ShutdownPhase::Final, || Err("boom".to_string()))
            .await;

        let report = mgr.shutdown().await;
        assert_eq!(report.hooks_succeeded, 2);
        assert_eq!(report.hooks_failed, 2);
    }

    // 15. Clone shares state
    #[tokio::test]
    async fn test_clone_shares_state() {
        let mgr1 = manager();
        let mgr2 = mgr1.clone();

        mgr1.on_shutdown("shared", ShutdownPhase::Cleanup, || Ok(()))
            .await;

        assert_eq!(mgr2.hook_count().await, 1);

        mgr2.shutdown().await;
        assert!(mgr1.is_shutting_down().await);
    }

    // 16. PreDrain hooks run before Cleanup
    #[tokio::test]
    async fn test_predrain_before_cleanup() {
        let mgr = manager();
        let val = Arc::new(std::sync::Mutex::new(0u32));
        let val1 = val.clone();
        let val2 = val.clone();

        mgr.on_shutdown("cleanup", ShutdownPhase::Cleanup, move || {
            let v = *val1.lock().unwrap();
            // PreDrain should have already set this to 1
            assert_eq!(v, 1);
            Ok(())
        })
        .await;

        mgr.on_shutdown("predrain", ShutdownPhase::PreDrain, move || {
            *val2.lock().unwrap() = 1;
            Ok(())
        })
        .await;

        let report = mgr.shutdown().await;
        assert_eq!(report.hooks_succeeded, 2);
    }
}
