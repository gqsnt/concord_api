client C { part p(x: u32) { query { x; } } scope s(..p) { GET g(); } }
