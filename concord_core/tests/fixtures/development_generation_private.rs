use concord_core::__development::CredentialGenerationSnapshot;

fn inspect(snapshot: CredentialGenerationSnapshot) -> u64 {
    snapshot.0
}

fn main() {
    let _ = CredentialGenerationSnapshot(7);
    let _ = inspect;
}
