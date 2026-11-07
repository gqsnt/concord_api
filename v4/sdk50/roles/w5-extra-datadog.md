## Slice-specific notes

The API host depends on the caller's Datadog site (region), chosen at run time. Reads can require
both an API key and an application key, each in its own header; wrong-region credentials or a
missing application key must surface as authentication errors without exposing either key.
