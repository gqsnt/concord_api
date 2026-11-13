type ProbeProblem = TgProblem;
model!(ChatFullInfo);
model!(ChatId);
model!(File);
model!(InlineKeyboardMarkup);
model!(InputFile);
model!(LinkPreviewOptions);
model!(Message);
model!(MessageEntity);
model!(ParseMode);
/// The typed success envelope.
pub type Reply<T> = StubReply<T>;
model!(ReplyMarkup);
model!(ReplyParameters);
model!(TgProblem);
model!(Update);
model!(UpdateKind);
model!(User);
/// Inert middleware `Quota`.
pub type Quota = Stub;
/// Inert middleware `Cooldown`.
pub type Cooldown = Stub;
