concord::api! {
    client Tasks {
        METHOD "LOCK/2" acquire(lock_id: LockId);
    }
}
fn main() {}
