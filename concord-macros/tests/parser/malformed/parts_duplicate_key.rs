client C { part p() { query { "k" = 1; } } part q() { query { "k" = 2; } } GET g(..p, ..q); }
