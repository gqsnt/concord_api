type ProbeProblem = SlackProblem;
model!(Acknowledged);
model!(Attachment);
model!(AuthIdentity);
model!(Block);
model!(ChannelInfo);
model!(CompletedFiles);
model!(Conversation);
model!(DeletedMessage);
model!(Message);
model!(MessageReceipt);
model!(Modal);
model!(OpenedView);
model!(SlackPage<T>);
model!(SlackProblem);
model!(UploadTicket);
model!(UploadedFile);
model!(User);
model!(UserInfo);
/// Inert middleware `Quota`.
pub type Quota = Stub;
/// Inert middleware `Cooldown`.
pub type Cooldown = Stub;
