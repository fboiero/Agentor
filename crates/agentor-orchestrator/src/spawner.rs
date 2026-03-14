use crate::task_queue::TaskQueue;
use crate::types::{AgentRole, Task};
use agentor_core::{AgentorError, AgentorResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A request to spawn a new sub-agent task under an existing parent task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Human-readable description of the sub-task.
    pub description: String,
    /// Role to assign to the sub-agent.
    pub role: AgentRole,
    /// The parent task that is spawning this sub-task.
    pub parent_task_id: Uuid,
    /// Other task IDs this sub-task depends on before it can execute.
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
}

/// Spawns sub-agent tasks within the orchestration task queue,
/// enforcing depth and fan-out limits to prevent runaway recursion.
pub struct SubAgentSpawner {
    /// Maximum allowed depth in the task hierarchy (root = 0).
    max_depth: u32,
    /// Maximum number of direct children a single task may have.
    max_children_per_task: u32,
    /// Shared reference to the task queue.
    queue: Arc<RwLock<TaskQueue>>,
}

impl SubAgentSpawner {
    /// Create a new spawner with default limits (max_depth=3, max_children=5).
    pub fn new(queue: Arc<RwLock<TaskQueue>>) -> Self {
        Self {
            max_depth: 3,
            max_children_per_task: 5,
            queue,
        }
    }

    /// Set the maximum depth for spawned sub-tasks.
    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set the maximum number of children a single task may spawn.
    pub fn with_max_children(mut self, max: u32) -> Self {
        self.max_children_per_task = max;
        self
    }

    /// Spawn a new sub-task under the given parent.
    ///
    /// Returns the ID of the newly created task, or an error if:
    /// - The parent task does not exist.
    /// - The depth limit would be exceeded.
    /// - The parent already has the maximum number of children.
    pub async fn spawn(&self, request: SpawnRequest) -> AgentorResult<Uuid> {
        // --- Read phase: validate parent and limits ---
        let (parent_depth, children_count) = {
            let queue = self.queue.read().await;

            let parent = queue.get(request.parent_task_id).ok_or_else(|| {
                AgentorError::Orchestrator(format!(
                    "parent task {} not found",
                    request.parent_task_id
                ))
            })?;

            let parent_depth = parent.depth;

            // Validate depth limit
            if parent_depth + 1 > self.max_depth {
                return Err(AgentorError::Orchestrator(format!(
                    "maximum spawn depth {} exceeded (parent depth is {})",
                    self.max_depth, parent_depth
                )));
            }

            // Count existing children of this parent
            let children_count = queue
                .all_tasks()
                .iter()
                .filter(|t| t.parent_task == Some(request.parent_task_id))
                .count();

            (parent_depth, children_count)
        };

        // Validate children limit
        if children_count >= self.max_children_per_task as usize {
            return Err(AgentorError::Orchestrator(format!(
                "parent task {} already has {} children (max {})",
                request.parent_task_id, children_count, self.max_children_per_task
            )));
        }

        // --- Write phase: create and insert the new task ---
        let mut task =
            Task::new(&request.description, request.role).with_dependencies(request.depends_on);
        task.parent_task = Some(request.parent_task_id);
        task.depth = parent_depth + 1;

        let task_id = task.id;

        {
            let mut queue = self.queue.write().await;
            queue.add(task);
        }

        Ok(task_id)
    }

    /// Return the IDs of all direct children of the given parent task.
    pub async fn children_of(&self, parent_id: Uuid) -> Vec<Uuid> {
        let queue = self.queue.read().await;
        queue
            .all_tasks()
            .iter()
            .filter(|t| t.parent_task == Some(parent_id))
            .map(|t| t.id)
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::types::AgentRole;

    /// Helper: create a shared queue with one root task and return (queue, root_id).
    fn setup_queue_with_root() -> (Arc<RwLock<TaskQueue>>, Uuid) {
        let mut tq = TaskQueue::new();
        let root = Task::new("Root task", AgentRole::Orchestrator);
        let root_id = root.id;
        tq.add(root);
        (Arc::new(RwLock::new(tq)), root_id)
    }

    #[tokio::test]
    async fn test_spawn_sub_task_successfully() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue));

        let request = SpawnRequest {
            description: "Implement auth module".to_string(),
            role: AgentRole::Coder,
            parent_task_id: root_id,
            depends_on: vec![],
        };

        let child_id = spawner.spawn(request).await.unwrap();

        let q = queue.read().await;
        let child = q.get(child_id).unwrap();
        assert_eq!(child.description, "Implement auth module");
        assert_eq!(child.assigned_to, AgentRole::Coder);
        assert_eq!(child.parent_task, Some(root_id));
        assert_eq!(child.depth, 1);
    }

    #[tokio::test]
    async fn test_depth_limit_prevents_too_deep_spawning() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue)).with_max_depth(2);

        // Spawn depth-1 child
        let child1_id = spawner
            .spawn(SpawnRequest {
                description: "Level 1".to_string(),
                role: AgentRole::Spec,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // Spawn depth-2 child
        let child2_id = spawner
            .spawn(SpawnRequest {
                description: "Level 2".to_string(),
                role: AgentRole::Coder,
                parent_task_id: child1_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // Depth-3 should fail because max_depth=2
        let result = spawner
            .spawn(SpawnRequest {
                description: "Level 3 (too deep)".to_string(),
                role: AgentRole::Tester,
                parent_task_id: child2_id,
                depends_on: vec![],
            })
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("maximum spawn depth"),
            "unexpected error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_children_limit_prevents_too_many_children() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue)).with_max_children(2);

        // Spawn child 1
        spawner
            .spawn(SpawnRequest {
                description: "Child 1".to_string(),
                role: AgentRole::Coder,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // Spawn child 2
        spawner
            .spawn(SpawnRequest {
                description: "Child 2".to_string(),
                role: AgentRole::Tester,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // Child 3 should fail
        let result = spawner
            .spawn(SpawnRequest {
                description: "Child 3 (over limit)".to_string(),
                role: AgentRole::Reviewer,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("already has 2 children"),
            "unexpected error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_children_of_returns_correct_children() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue));

        let c1 = spawner
            .spawn(SpawnRequest {
                description: "Child A".to_string(),
                role: AgentRole::Spec,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        let c2 = spawner
            .spawn(SpawnRequest {
                description: "Child B".to_string(),
                role: AgentRole::Coder,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // Spawn a grandchild under c1 -- should NOT appear in root's children
        let _gc = spawner
            .spawn(SpawnRequest {
                description: "Grandchild".to_string(),
                role: AgentRole::Tester,
                parent_task_id: c1,
                depends_on: vec![],
            })
            .await
            .unwrap();

        let children = spawner.children_of(root_id).await;
        assert_eq!(children.len(), 2);
        assert!(children.contains(&c1));
        assert!(children.contains(&c2));
    }

    #[tokio::test]
    async fn test_parent_task_and_depth_fields_set_correctly() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue));

        // depth 1
        let d1_id = spawner
            .spawn(SpawnRequest {
                description: "Depth 1".to_string(),
                role: AgentRole::Spec,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        // depth 2
        let d2_id = spawner
            .spawn(SpawnRequest {
                description: "Depth 2".to_string(),
                role: AgentRole::Coder,
                parent_task_id: d1_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        let q = queue.read().await;

        let root = q.get(root_id).unwrap();
        assert_eq!(root.depth, 0);
        assert_eq!(root.parent_task, None);

        let d1 = q.get(d1_id).unwrap();
        assert_eq!(d1.depth, 1);
        assert_eq!(d1.parent_task, Some(root_id));

        let d2 = q.get(d2_id).unwrap();
        assert_eq!(d2.depth, 2);
        assert_eq!(d2.parent_task, Some(d1_id));
    }

    #[tokio::test]
    async fn test_spawn_with_dependencies() {
        let (queue, root_id) = setup_queue_with_root();
        let spawner = SubAgentSpawner::new(Arc::clone(&queue));

        // Spawn two children
        let spec_id = spawner
            .spawn(SpawnRequest {
                description: "Write spec".to_string(),
                role: AgentRole::Spec,
                parent_task_id: root_id,
                depends_on: vec![],
            })
            .await
            .unwrap();

        let code_id = spawner
            .spawn(SpawnRequest {
                description: "Write code".to_string(),
                role: AgentRole::Coder,
                parent_task_id: root_id,
                depends_on: vec![spec_id],
            })
            .await
            .unwrap();

        // Spawn a test task that depends on both spec and code
        let test_id = spawner
            .spawn(SpawnRequest {
                description: "Write tests".to_string(),
                role: AgentRole::Tester,
                parent_task_id: root_id,
                depends_on: vec![spec_id, code_id],
            })
            .await
            .unwrap();

        let q = queue.read().await;
        let test_task = q.get(test_id).unwrap();
        assert_eq!(test_task.dependencies.len(), 2);
        assert!(test_task.dependencies.contains(&spec_id));
        assert!(test_task.dependencies.contains(&code_id));
        assert_eq!(test_task.parent_task, Some(root_id));

        // The test task should NOT be ready (deps not completed)
        assert!(!test_task.is_ready(&[]));
        // It should be ready when both deps are completed
        assert!(test_task.is_ready(&[spec_id, code_id]));
    }
}
