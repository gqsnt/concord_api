POST create(task: NewTask) -> Created {
    body json(task);
    status { 2xx { body, headers } => created(body, headers)? }
}
