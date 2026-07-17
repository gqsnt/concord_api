use std::sync::Arc;

use concord_core::advanced::{DebugLevel, RuntimeConfigBuilder, RuntimeHooks};

struct Hooks;
impl RuntimeHooks for Hooks {}

#[test]
fn final_runtime_hook_and_debug_setter_signatures_are_public() {
    let _hooks_setter: fn(RuntimeConfigBuilder, Arc<dyn RuntimeHooks>) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::hooks;
    let _debug_setter: fn(RuntimeConfigBuilder, DebugLevel) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::debug;
    let hooks: Arc<dyn RuntimeHooks> = Arc::new(Hooks);
    assert_eq!(Arc::strong_count(&hooks), 1);
}
