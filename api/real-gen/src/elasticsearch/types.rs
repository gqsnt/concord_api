type ProbeProblem = EsProblem;
model!(BulkReply);
model!(CatFormat);
model!(CatReply);
model!(ClosedPit);
model!(CountReply);
model!(EsProblem);
model!(GetReply);
model!(IndexAck);
model!(OpenedPit);
model!(PitCursor);
model!(Refresh);
model!(SearchPage);
model!(WriteReply);
/// Inert middleware `Quota`.
pub type Quota = Stub;
/// Inert middleware `Cooldown`.
pub type Cooldown = Stub;
