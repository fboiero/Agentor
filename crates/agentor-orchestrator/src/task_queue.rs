use crate::types::{Task, TaskStatus};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// A task queue with dependency resolution (topological ordering).
pub struct TaskQueue {
    tasks: HashMap<Uuid, Task>,
    completed: Vec<Uuid>,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            completed: Vec::new(),
        }
    }

    /// Add a task to the queue.
    pub fn add(&mut self, task: Task) -> Uuid {
        let id = task.id;
        self.tasks.insert(id, task);
        id
    }

    /// Get the next ready task (all dependencies resolved, status == Pending).
    /// Returns tasks in creation-time order among those that are ready.
    pub fn next_ready(&self) -> Option<&Task> {
        let mut ready: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.is_ready(&self.completed))
            .collect();
        ready.sort_by_key(|t| t.created_at);
        ready.into_iter().next()
    }

    /// Get all tasks that are ready to execute.
    pub fn all_ready(&self) -> Vec<&Task> {
        let mut ready: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.is_ready(&self.completed))
            .collect();
        ready.sort_by_key(|t| t.created_at);
        ready
    }

    /// Mark a task as running.
    pub fn mark_running(&mut self, id: Uuid) -> bool {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.status = TaskStatus::Running;
            true
        } else {
            false
        }
    }

    /// Mark a task as completed.
    pub fn mark_completed(&mut self, id: Uuid) -> bool {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.status = TaskStatus::Completed;
            task.completed_at = Some(Utc::now());
            self.completed.push(id);
            true
        } else {
            false
        }
    }

    /// Mark a task as failed.
    pub fn mark_failed(&mut self, id: Uuid, reason: String) -> bool {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.status = TaskStatus::Failed { reason };
            true
        } else {
            false
        }
    }

    /// Mark a task as needing human review (HITL).
    pub fn mark_needs_review(&mut self, id: Uuid) -> bool {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.status = TaskStatus::NeedsHumanReview;
            true
        } else {
            false
        }
    }

    /// Get a task by ID.
    pub fn get(&self, id: Uuid) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Get a mutable reference to a task.
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// List all tasks.
    pub fn all_tasks(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.values().collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    /// Count of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }

    /// Count of completed tasks.
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }

    /// Total number of tasks.
    pub fn total_count(&self) -> usize {
        self.tasks.len()
    }

    /// Check if all tasks are in a terminal state (completed, failed, or awaiting review).
    pub fn is_done(&self) -> bool {
        self.tasks.values().all(|t| {
            matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Failed { .. } | TaskStatus::NeedsHumanReview
            )
        })
    }

    /// Count of tasks awaiting human review.
    pub fn needs_review_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::NeedsHumanReview)
            .count()
    }

    /// Check for cycles in the dependency graph.
    /// Returns true if a cycle is detected.
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashMap::new();
        for &id in self.tasks.keys() {
            if self.dfs_cycle(id, &mut visited) {
                return true;
            }
        }
        false
    }

    fn dfs_cycle(&self, id: Uuid, visited: &mut HashMap<Uuid, u8>) -> bool {
        match visited.get(&id) {
            Some(1) => return true,  // back edge = cycle
            Some(2) => return false, // already processed
            _ => {}
        }
        visited.insert(id, 1); // mark as in progress
        if let Some(task) = self.tasks.get(&id) {
            for dep in &task.dependencies {
                if self.dfs_cycle(*dep, visited) {
                    return true;
                }
            }
        }
        visited.insert(id, 2); // mark as done
        false
    }
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentRole;

    #[test]
    fn test_empty_queue() {
        let queue = TaskQueue::new();
        assert_eq!(queue.total_count(), 0);
        assert_eq!(queue.pending_count(), 0);
        assert!(queue.is_done());
        assert!(queue.next_ready().is_none());
    }

    #[test]
    fn test_add_and_retrieve() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Test task", AgentRole::Coder);
        let id = task.id;
        queue.add(task);

        assert_eq!(queue.total_count(), 1);
        assert!(queue.get(id).is_some());
        assert_eq!(queue.get(id).unwrap().description, "Test task");
    }

    #[test]
    fn test_next_ready_no_deps() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Ready task", AgentRole::Spec);
        queue.add(task);

        let ready = queue.next_ready();
        assert!(ready.is_some());
        assert_eq!(ready.unwrap().description, "Ready task");
    }

    #[test]
    fn test_next_ready_with_deps() {
        let mut queue = TaskQueue::new();

        let t1 = Task::new("First", AgentRole::Spec);
        let t1_id = t1.id;
        queue.add(t1);

        let t2 = Task::new("Second", AgentRole::Coder).with_dependencies(vec![t1_id]);
        queue.add(t2);

        // Only t1 should be ready
        let ready = queue.next_ready();
        assert_eq!(ready.unwrap().description, "First");

        // Complete t1
        queue.mark_running(t1_id);
        queue.mark_completed(t1_id);

        // Now t2 should be ready
        let ready = queue.next_ready();
        assert_eq!(ready.unwrap().description, "Second");
    }

    #[test]
    fn test_all_ready_parallel() {
        let mut queue = TaskQueue::new();

        let t1 = Task::new("Task A", AgentRole::Spec);
        queue.add(t1);
        let t2 = Task::new("Task B", AgentRole::Coder);
        queue.add(t2);
        let t3 = Task::new("Task C", AgentRole::Tester);
        queue.add(t3);

        let ready = queue.all_ready();
        assert_eq!(ready.len(), 3);
    }

    #[test]
    fn test_mark_completed() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Complete me", AgentRole::Coder);
        let id = task.id;
        queue.add(task);

        queue.mark_running(id);
        assert_eq!(queue.get(id).unwrap().status, TaskStatus::Running);

        queue.mark_completed(id);
        assert_eq!(queue.get(id).unwrap().status, TaskStatus::Completed);
        assert_eq!(queue.completed_count(), 1);
        assert!(queue.is_done());
    }

    #[test]
    fn test_mark_failed() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Fail me", AgentRole::Tester);
        let id = task.id;
        queue.add(task);

        queue.mark_failed(id, "compilation error".to_string());
        assert!(matches!(
            queue.get(id).unwrap().status,
            TaskStatus::Failed { .. }
        ));
    }

    #[test]
    fn test_mark_needs_review() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Review me", AgentRole::Reviewer);
        let id = task.id;
        queue.add(task);

        queue.mark_needs_review(id);
        assert_eq!(queue.get(id).unwrap().status, TaskStatus::NeedsHumanReview);
    }

    #[test]
    fn test_dependency_chain() {
        let mut queue = TaskQueue::new();

        let t1 = Task::new("Spec", AgentRole::Spec);
        let t1_id = t1.id;
        queue.add(t1);

        let t2 = Task::new("Code", AgentRole::Coder).with_dependencies(vec![t1_id]);
        let t2_id = t2.id;
        queue.add(t2);

        let t3 = Task::new("Test", AgentRole::Tester).with_dependencies(vec![t2_id]);
        let t3_id = t3.id;
        queue.add(t3);

        let t4 = Task::new("Review", AgentRole::Reviewer).with_dependencies(vec![t2_id, t3_id]);
        queue.add(t4);

        // Only t1 ready initially
        assert_eq!(queue.all_ready().len(), 1);

        queue.mark_running(t1_id);
        queue.mark_completed(t1_id);
        // Now t2 ready
        assert_eq!(queue.all_ready().len(), 1);

        queue.mark_running(t2_id);
        queue.mark_completed(t2_id);
        // Now t3 ready
        assert_eq!(queue.all_ready().len(), 1);

        queue.mark_running(t3_id);
        queue.mark_completed(t3_id);
        // Now t4 ready (both deps complete)
        assert_eq!(queue.all_ready().len(), 1);
    }

    #[test]
    fn test_no_cycle() {
        let mut queue = TaskQueue::new();
        let t1 = Task::new("A", AgentRole::Spec);
        let t1_id = t1.id;
        queue.add(t1);
        let t2 = Task::new("B", AgentRole::Coder).with_dependencies(vec![t1_id]);
        queue.add(t2);
        assert!(!queue.has_cycle());
    }

    #[test]
    fn test_cycle_detection() {
        let mut queue = TaskQueue::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut t1 = Task::new("A", AgentRole::Spec);
        t1.id = id1;
        t1.dependencies = vec![id2];

        let mut t2 = Task::new("B", AgentRole::Coder);
        t2.id = id2;
        t2.dependencies = vec![id1];

        queue.add(t1);
        queue.add(t2);
        assert!(queue.has_cycle());
    }

    #[test]
    fn test_is_done() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Only task", AgentRole::Spec);
        let id = task.id;
        queue.add(task);

        assert!(!queue.is_done());
        queue.mark_completed(id);
        assert!(queue.is_done());
    }

    #[test]
    fn test_is_done_with_needs_review() {
        let mut queue = TaskQueue::new();
        let t1 = Task::new("Task 1", AgentRole::Coder);
        let t1_id = t1.id;
        let t2 = Task::new("Task 2", AgentRole::Reviewer);
        let t2_id = t2.id;
        queue.add(t1);
        queue.add(t2);

        queue.mark_completed(t1_id);
        assert!(!queue.is_done());

        queue.mark_needs_review(t2_id);
        assert!(queue.is_done());
    }

    #[test]
    fn test_is_done_with_failed() {
        let mut queue = TaskQueue::new();
        let task = Task::new("Failing task", AgentRole::Tester);
        let id = task.id;
        queue.add(task);

        queue.mark_failed(id, "error".into());
        assert!(queue.is_done());
    }

    #[test]
    fn test_needs_review_count() {
        let mut queue = TaskQueue::new();
        let t1 = Task::new("Task 1", AgentRole::Coder);
        let t1_id = t1.id;
        let t2 = Task::new("Task 2", AgentRole::Reviewer);
        let t2_id = t2.id;
        queue.add(t1);
        queue.add(t2);

        assert_eq!(queue.needs_review_count(), 0);
        queue.mark_needs_review(t1_id);
        assert_eq!(queue.needs_review_count(), 1);
        queue.mark_needs_review(t2_id);
        assert_eq!(queue.needs_review_count(), 2);
    }
}
