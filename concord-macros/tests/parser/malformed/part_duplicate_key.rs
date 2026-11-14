client C { part p() { query { "k" = 1; } } GET g(..p) { query { override "k" = 2; } } }
