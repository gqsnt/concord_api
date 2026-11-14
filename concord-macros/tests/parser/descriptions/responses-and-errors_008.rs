POST create(task: NewTask) -> Created {
    body json(task);
    status { 201 { body, headers } => created(body, headers)? }
}
