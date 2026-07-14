use concord_core::__development::{
    CapturedNativeRequest, CredentialGenerationSnapshot, CredentialLifecycleEvent,
    DeterministicNativeExecutor, ScriptedNativeResponse, UnsafeCredentialPlacementExpectations,
};

fn inspect(event: CredentialLifecycleEvent) {
    if let CredentialLifecycleEvent::GenerationInvalidated {
        requested, current, ..
    } = event
    {
        let requested_clone = requested.clone();
        let _same_identity = requested == current;
        if let Some(snapshot) = requested_clone {
            let _ = format!("{snapshot:?}");
            let _ = snapshot.to_string();
        }
    }
}

fn main() {
    let _ = core::mem::size_of::<CredentialGenerationSnapshot>();
    let _ = core::mem::size_of::<CredentialLifecycleEvent>();
    let _ = core::mem::size_of::<CapturedNativeRequest>();
    let _ = core::mem::size_of::<DeterministicNativeExecutor>();
    let _ = core::mem::size_of::<ScriptedNativeResponse>();
    let _ = core::mem::size_of::<UnsafeCredentialPlacementExpectations>();
    let _ = inspect as fn(CredentialLifecycleEvent);
}
