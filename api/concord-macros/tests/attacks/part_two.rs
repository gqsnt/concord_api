client C { part p(x: u32) { query { x; } } part q(y: u32) { query { y; } } GET g(..p, ..q); }
