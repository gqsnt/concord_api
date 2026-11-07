## Slice-specific notes

The same filtered query is served as GeoJSON or CSV (`format=`); the CSV export must be processed
incrementally (rows as they arrive) with quoted commas/newlines preserved and bad rows never
silently skipped. Record in `FRICTION.md` how you expressed "the same query in two formats"
(declaration, runtime choice, separate endpoints) and what it cost.
