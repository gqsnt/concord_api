Add the FDSN event filters `minmagnitude`, `maxmagnitude` and `orderby` (`time`, `time-asc`,
`magnitude`, `magnitude-asc`) to the slice (USGS FDSN event web service documentation; cite it).
They must apply identically to the GeoJSON query and the CSV export of the same filtered window,
validated where the documentation states constraints (e.g. min ≤ max), and the CSV export must
remain incremental.
