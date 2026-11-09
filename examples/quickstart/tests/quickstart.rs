//! The quick start runs against the local stand-in and sends exactly what `docs/QUICKSTART.md`
//! describes.

use concord_example_quickstart::local_server::{LocalServer, tasks_service};

#[tokio::test]
async fn quick_start_sends_the_documented_requests() {
    let server = LocalServer::start(tasks_service("t0k3n")).await.unwrap();
    concord_example_quickstart::run(&format!("{}/api", server.url()), "t0k3n".into())
        .await
        .unwrap();

    let requests = server.requests();
    let lines: Vec<String> = requests
        .iter()
        .map(|r| format!("{} {}", r.method, r.target))
        .collect();
    assert_eq!(
        lines,
        [
            "POST /api/projects/demo/tasks",
            "GET /api/projects/demo/tasks?done=false&limit=20",
            "POST /api/projects/demo/tasks",
        ]
    );
    for request in &requests {
        assert_eq!(request.header("authorization"), Some("Bearer t0k3n"));
    }
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[0].body, br#"{"title":"Write the quick start"}"#);
    assert_eq!(requests[2].body, br#"{"title":""}"#);
}

#[tokio::test]
async fn a_rejected_token_reaches_the_caller_as_a_status_error() {
    let server = LocalServer::start(tasks_service("t0k3n")).await.unwrap();
    let error = concord_example_quickstart::run(&format!("{}/api", server.url()), "wrong".into())
        .await
        .unwrap_err();
    let error = error
        .downcast_ref::<concord::Error<concord_example_quickstart::Problem>>()
        .expect("the first call's Concord error");
    assert_eq!(error.kind(), concord::ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
    assert_eq!(error.endpoint(), Some("Tasks.project.create"));
    assert!(error.provider().is_some());
    // Concord's own rendering never contains the token.
    assert_eq!(
        error.to_string(),
        "POST Tasks.project.create: HTTP status 401 Unauthorized"
    );
    assert_eq!(server.requests().len(), 1);
}

/// The generated signatures listed in `docs/QUICKSTART.md`, checked by coercion to function
/// pointers (arguments written as `impl IntoValue<T>` are instantiated with `T`).
#[test]
fn the_documented_signatures() {
    use concord::{Error, Prepared, reqwest};
    use concord_example_quickstart::*;

    let _: fn(reqwest::Client, TasksConfig) -> Tasks = Tasks::new;
    let _: fn(Tasks, &'static str) -> Result<Tasks, Error> = Tasks::with_base_url;
    let _: fn(Tasks, String) -> Tasks = Tasks::set_token;
    let _: fn(&Tasks, String) -> TasksProject = Tasks::project;
    let _: fn(&TasksProject) -> TasksProjectTasks = TasksProject::tasks;
    let _: fn(&TasksProject, NewTask) -> TasksProjectCreate = TasksProject::create;
    let _: fn(TasksProjectTasks, bool) -> TasksProjectTasks = TasksProjectTasks::set_done;
    let _: fn(TasksProjectTasks, Option<bool>) -> TasksProjectTasks = TasksProjectTasks::maybe_done;
    let _: fn(TasksProjectTasks, u32) -> TasksProjectTasks = TasksProjectTasks::set_limit;
    type PreparedTasks = Result<Prepared<Vec<Task>, Problem>, Error>;
    let _: fn(TasksProjectTasks) -> PreparedTasks = TasksProjectTasks::prepare;
    let _: fn(TasksProjectCreate) -> Result<Prepared<Task, Problem>, Error> =
        TasksProjectCreate::prepare;
}
