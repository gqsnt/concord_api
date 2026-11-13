GET g(flag: bool) { idempotent if *flag { true } else { false }; body make(); }
