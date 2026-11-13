type ProbeProblem = Status;
model!(CronTab);
model!(DeleteOptions);
model!(Deployment);
model!(KubeList<T>);
model!(Pod);
model!(Scale);
model!(Status);
/// Inert middleware `Quota`.
pub type Quota = Stub;
/// Inert middleware `Cooldown`.
pub type Cooldown = Stub;
