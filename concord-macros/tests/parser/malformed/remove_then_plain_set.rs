client C { query { "k" = 1; } scope s { query { remove "k"; } GET g() { query { "k" = 2; } } } }
