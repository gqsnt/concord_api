#[derive(concord::WireDisplay)]
#[wire_display(runtime = concord)]
struct UnknownOption;

#[derive(concord::WireDisplay)]
#[wire_display(crate = concord)]
#[wire_display(crate = concord)]
struct DuplicatePath;

#[derive(concord::WireDisplay)]
#[wire_display(crate = "concord")]
struct StringPath;

fn main() {}
