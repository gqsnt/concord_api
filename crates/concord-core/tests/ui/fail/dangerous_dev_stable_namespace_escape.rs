use concord_core::advanced::DeterministicExecutor as AdvancedExecutor;
use concord_core::prelude::DeterministicExecutor as PreludeExecutor;

fn main() {
    let _ = (AdvancedExecutor::new(), PreludeExecutor::new());
}
