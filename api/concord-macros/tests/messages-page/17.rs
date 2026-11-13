concord::api! {
client Tasks { base "https://a.example"; POST create(task: Task) { body json(task); body json(task); } }
}
fn main() {}
