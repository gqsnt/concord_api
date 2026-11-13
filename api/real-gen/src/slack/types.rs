type ProbeProblem = SlackProblem;
model!(Block);
model!(ChannelId);
model!(ChannelInfo);
model!(ChannelList);
model!(Completed);
model!(ConversationType);
model!(Deleted);
model!(Done);
model!(FileRef);
model!(Identity);
model!(MessageList);
model!(Metadata);
model!(Opened);
model!(Posted);
/// The typed success envelope.
pub type Reply<T> = StubReply<T>;
model!(SlackProblem);
model!(TeamId);
model!(Ts);
model!(Updated);
model!(UploadTicket);
model!(UserId);
model!(UserInfo);
model!(UserList);
model!(View);
/// Inert middleware `Quota`.
pub type Quota = Stub;
/// Inert middleware `Cooldown`.
pub type Cooldown = Stub;
