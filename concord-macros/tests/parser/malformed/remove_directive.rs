client C { query { "old" = 1; } GET g() { query { remove "old"; } } }
