FRED series observations accept `units` (lin, chg, ch1, pch, pc1, pca, cch, cca, log) and
`frequency` aggregation (d, w, bw, m, q, sa, a, plus the documented weekly variants) with
`aggregation_method` (avg, sum, eop) (FRED series/observations documentation; cite it). Add them so
they apply identically to the JSON, XML and zipped-CSV forms of observations, validated against the
documented value sets, and keep missing-value strings (".") distinct from numbers in every form.
