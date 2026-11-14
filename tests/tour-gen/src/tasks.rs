concord::api! {
/// A client for creating, finding, and deleting tasks.
///
/// # Example
///
/// ```no_run
/// use tour::Tasks;
///
/// # async fn example() -> tour::tasks::Result<()> {
/// let tasks = Tasks::new();
/// let open = tasks.list().done(false).await?;
/// # Ok(())
/// # }
/// ```
///
pub client Tasks {
    base "https://tasks.example.com/api/tasks";
    read Json;
    /// List tasks in their current order.
    GET list(
        /// Filter tasks by completion state.
        done?: bool,
    ) -> Vec<Task> {
        query { done; }
    }
    /// Create a task with a title.
    POST create(
        /// The task to create.
        task: NewTask,
    ) -> Task {
        body json(task);
    }
    /// Fetch one task by its identifier.
    GET get(
        /// The identifier of the task to fetch.
        id: u64,
    ) -> Task {
        path "{id}";
    }
    /// Delete one task by its identifier.
    DELETE delete(
        /// The identifier of the task to delete.
        id: u64,
    ) {
        path "{id}";
    }
}
}
