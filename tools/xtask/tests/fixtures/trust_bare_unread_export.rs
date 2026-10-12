#[macro_export]
macro_rules! api { ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); }; }
