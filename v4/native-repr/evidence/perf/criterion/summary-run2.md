| benchmark | mean | 95% CI (mean) | throughput | a915c8d mean | change (means) |
|---|---:|---:|---:|---:|---:|
| `body_encode/bytes/100B/concord/input` | 9.5 ns | 9.4 ns – 9.5 ns | 10053.0 MiB/s | 10.3 ns | -7.5% |
| `body_encode/bytes/100B/concord/prepare` | 208.6 ns | 207.8 ns – 209.5 ns | 457.2 MiB/s | 184.2 ns | +13.3% |
| `body_encode/bytes/100B/manual/input` | 8.2 ns | 8.1 ns – 8.2 ns | 11692.8 MiB/s | 8.2 ns | -0.8% |
| `body_encode/bytes/100B/manual/prepare` | 147.8 ns | 147.3 ns – 148.5 ns | 645.3 MiB/s | 149.1 ns | -0.9% |
| `body_encode/bytes/10KB/concord/input` | 9.5 ns | 9.5 ns – 9.6 ns | 1024532.8 MiB/s | 10.2 ns | -6.9% |
| `body_encode/bytes/10KB/concord/prepare` | 207.4 ns | 206.9 ns – 208.0 ns | 47080.8 MiB/s | 185.7 ns | +11.7% |
| `body_encode/bytes/10KB/manual/input` | 8.2 ns | 8.2 ns – 8.2 ns | 1189651.2 MiB/s | 8.3 ns | -1.2% |
| `body_encode/bytes/10KB/manual/prepare` | 147.4 ns | 147.0 ns – 147.8 ns | 66274.8 MiB/s | 149.4 ns | -1.4% |
| `body_encode/bytes/1MB/concord/input` | 9.5 ns | 9.5 ns – 9.6 ns | 104857111.3 MiB/s | 10.3 ns | -7.0% |
| `body_encode/bytes/1MB/concord/prepare` | 209.3 ns | 208.2 ns – 210.3 ns | 4777947.7 MiB/s | 190.3 ns | +10.0% |
| `body_encode/bytes/1MB/manual/input` | 8.2 ns | 8.2 ns – 8.2 ns | 121992895.0 MiB/s | 8.2 ns | -0.2% |
| `body_encode/bytes/1MB/manual/prepare` | 147.8 ns | 147.1 ns – 148.5 ns | 6764993.1 MiB/s | 151.5 ns | -2.4% |
| `body_encode/json/100B/concord/input` | 88.3 ns | 88.0 ns – 88.7 ns | 1965.5 MiB/s | 89.1 ns | -0.9% |
| `body_encode/json/100B/concord/prepare` | 483.7 ns | 482.8 ns – 485.0 ns | 358.8 MiB/s | 481.4 ns | +0.5% |
| `body_encode/json/100B/manual/input` | 87.9 ns | 87.6 ns – 88.2 ns | 1974.9 MiB/s | 89.7 ns | -2.0% |
| `body_encode/json/100B/manual/prepare` | 437.7 ns | 436.7 ns – 439.1 ns | 396.5 MiB/s | 430.5 ns | +1.7% |
| `body_encode/json/10KB/concord/input` | 4.489 µs | 4.478 µs – 4.501 µs | 2193.2 MiB/s | 4.509 µs | -0.5% |
| `body_encode/json/10KB/concord/prepare` | 12.509 µs | 12.483 µs – 12.544 µs | 787.0 MiB/s | 11.998 µs | +4.3% |
| `body_encode/json/10KB/manual/input` | 4.506 µs | 4.495 µs – 4.519 µs | 2184.8 MiB/s | 4.504 µs | +0.1% |
| `body_encode/json/10KB/manual/prepare` | 12.448 µs | 12.437 µs – 12.458 µs | 790.9 MiB/s | 11.338 µs | +9.8% |
| `body_encode/json/1MB/concord/input` | 421.918 µs | 421.183 µs – 422.707 µs | 2370.2 MiB/s | 421.867 µs | +0.0% |
| `body_encode/json/1MB/concord/prepare` | 1.154 ms | 1.153 ms – 1.155 ms | 866.9 MiB/s | 1.127 ms | +2.4% |
| `body_encode/json/1MB/manual/input` | 421.284 µs | 420.500 µs – 422.155 µs | 2373.8 MiB/s | 422.100 µs | -0.2% |
| `body_encode/json/1MB/manual/prepare` | 1.149 ms | 1.148 ms – 1.150 ms | 870.5 MiB/s | 1.035 ms | +11.0% |
| `body_encode/text/100B/concord/input` | 11.6 ns | 11.5 ns – 11.7 ns | 8235.2 MiB/s | 11.9 ns | -2.5% |
| `body_encode/text/100B/concord/prepare` | 194.5 ns | 194.0 ns – 195.3 ns | 490.2 MiB/s | 202.6 ns | -4.0% |
| `body_encode/text/100B/manual/input` | 10.8 ns | 10.7 ns – 10.9 ns | 8825.4 MiB/s | 10.8 ns | +0.2% |
| `body_encode/text/100B/manual/prepare` | 152.5 ns | 151.8 ns – 153.3 ns | 625.4 MiB/s | 152.7 ns | -0.1% |
| `body_encode/text/10KB/concord/input` | 439.0 ns | 408.5 ns – 469.6 ns | 22247.5 MiB/s | 441.0 ns | -0.5% |
| `body_encode/text/10KB/concord/prepare` | 202.6 ns | 201.1 ns – 204.4 ns | 48198.3 MiB/s | 210.3 ns | -3.6% |
| `body_encode/text/10KB/manual/input` | 404.9 ns | 379.6 ns – 429.7 ns | 24118.1 MiB/s | 399.1 ns | +1.4% |
| `body_encode/text/10KB/manual/prepare` | 160.6 ns | 159.3 ns – 162.3 ns | 60800.7 MiB/s | 160.4 ns | +0.2% |
| `body_encode/text/1MB/concord/input` | 38.915 µs | 37.194 µs – 40.637 µs | 25697.2 MiB/s | 42.271 µs | -7.9% |
| `body_encode/text/1MB/concord/prepare` | 298.4 ns | 294.7 ns – 302.1 ns | 3350671.2 MiB/s | 283.6 ns | +5.3% |
| `body_encode/text/1MB/manual/input` | 41.826 µs | 39.392 µs – 44.353 µs | 23908.6 MiB/s | 38.924 µs | +7.5% |
| `body_encode/text/1MB/manual/prepare` | 245.8 ns | 243.1 ns – 248.9 ns | 4068145.9 MiB/s | 245.8 ns | -0.0% |
| `compose/append/concord/input` | 9.4 ns | 9.3 ns – 9.4 ns | - | 9.4 ns | -1.0% |
| `compose/append/concord/prepare` | 443.7 ns | 442.5 ns – 445.6 ns | - | 533.0 ns | -16.8% |
| `compose/append/manual/input` | 10.0 ns | 10.0 ns – 10.1 ns | - | 10.2 ns | -1.3% |
| `compose/append/manual/prepare` | 391.2 ns | 390.7 ns – 392.1 ns | - | 386.9 ns | +1.1% |
| `compose/deep8_navigate/concord/input` | 299.7 ns | 299.0 ns – 300.4 ns | - | 300.2 ns | -0.2% |
| `compose/deep8_navigate/concord/input_prepare` | 1.765 µs | 1.761 µs – 1.769 µs | - | 1.918 µs | -8.0% |
| `compose/deep8_navigate/concord/prepare` | 1.557 µs | 1.549 µs – 1.565 µs | - | 1.696 µs | -8.2% |
| `compose/deep8_navigate/manual/input` | 188.2 ns | 187.8 ns – 188.7 ns | - | 186.8 ns | +0.8% |
| `compose/deep8_navigate/manual/input_prepare` | 1.215 µs | 1.212 µs – 1.219 µs | - | 1.182 µs | +2.8% |
| `compose/deep8_navigate/manual/prepare` | 1.023 µs | 1.021 µs – 1.026 µs | - | 1.006 µs | +1.7% |
| `compose/deep8_reused_scope/concord/input` | 5.8 ns | 5.8 ns – 5.8 ns | - | 5.8 ns | +0.2% |
| `compose/deep8_reused_scope/concord/input_prepare` | 1.345 µs | 1.343 µs – 1.348 µs | - | 1.487 µs | -9.6% |
| `compose/deep8_reused_scope/concord/prepare` | 1.345 µs | 1.342 µs – 1.348 µs | - | 1.492 µs | -9.9% |
| `compose/deep8_reused_scope/manual/input_prepare` | 478.3 ns | 476.8 ns – 480.0 ns | - | 487.0 ns | -1.8% |
| `compose/deep8_reused_scope/manual/prepare` | 481.1 ns | 479.7 ns – 482.7 ns | - | 485.7 ns | -0.9% |
| `compose/inherit/concord/input` | 5.8 ns | 5.8 ns – 5.8 ns | - | 5.8 ns | +0.0% |
| `compose/inherit/concord/prepare` | 412.1 ns | 409.9 ns – 414.9 ns | - | 481.3 ns | -14.4% |
| `compose/inherit/manual/input` | 0.8 ns | 0.7 ns – 0.8 ns | - | 1.0 ns | -23.9% |
| `compose/inherit/manual/prepare` | 342.2 ns | 341.8 ns – 342.5 ns | - | 337.2 ns | +1.5% |
| `compose/override_absent/concord/input` | 5.1 ns | 5.1 ns – 5.1 ns | - | 5.2 ns | -1.4% |
| `compose/override_absent/concord/prepare` | 414.5 ns | 413.6 ns – 415.7 ns | - | 485.2 ns | -14.6% |
| `compose/override_absent/manual/input` | 1.0 ns | 0.8 ns – 1.3 ns | - | 0.9 ns | +14.5% |
| `compose/override_absent/manual/prepare` | 344.9 ns | 344.4 ns – 345.6 ns | - | 338.0 ns | +2.0% |
| `compose/override_present/concord/input` | 13.0 ns | 13.0 ns – 13.0 ns | - | 13.0 ns | -0.3% |
| `compose/override_present/concord/prepare` | 417.0 ns | 415.7 ns – 418.8 ns | - | 482.6 ns | -13.6% |
| `compose/override_present/manual/input` | 10.0 ns | 10.0 ns – 10.0 ns | - | 10.1 ns | -1.2% |
| `compose/override_present/manual/prepare` | 351.0 ns | 350.5 ns – 351.8 ns | - | 344.7 ns | +1.8% |
| `compose/remove/concord/input` | 5.8 ns | 5.8 ns – 5.8 ns | - | 5.8 ns | +0.2% |
| `compose/remove/concord/prepare` | 380.4 ns | 379.3 ns – 382.0 ns | - | 440.0 ns | -13.5% |
| `compose/remove/manual/input` | 0.9 ns | 0.7 ns – 1.1 ns | - | 1.0 ns | -8.0% |
| `compose/remove/manual/prepare` | 299.1 ns | 298.8 ns – 299.6 ns | - | 294.6 ns | +1.5% |
| `construct/minimal/concord/input` | 5.9 ns | 5.9 ns – 5.9 ns | - | 5.9 ns | -0.1% |
| `construct/minimal/concord/input_prepare` | 142.0 ns | 141.6 ns – 142.5 ns | - | 142.3 ns | -0.2% |
| `construct/minimal/concord/prepare` | 140.0 ns | 139.7 ns – 140.4 ns | - | 138.5 ns | +1.1% |
| `construct/minimal/manual/input_prepare` | 102.9 ns | 102.6 ns – 103.3 ns | - | 102.5 ns | +0.5% |
| `construct/minimal/manual/prepare` | 102.5 ns | 102.1 ns – 102.9 ns | - | 102.3 ns | +0.2% |
| `construct/realistic_all_present/concord/input` | 104.7 ns | 104.4 ns – 105.0 ns | - | 104.9 ns | -0.2% |
| `construct/realistic_all_present/concord/input_prepare` | 1.447 µs | 1.443 µs – 1.453 µs | - | 1.529 µs | -5.3% |
| `construct/realistic_all_present/concord/prepare` | 1.401 µs | 1.394 µs – 1.409 µs | - | 1.550 µs | -9.6% |
| `construct/realistic_all_present/manual/input` | 91.5 ns | 91.2 ns – 91.9 ns | - | 91.4 ns | +0.2% |
| `construct/realistic_all_present/manual/input_prepare` | 822.9 ns | 818.5 ns – 829.7 ns | - | 826.9 ns | -0.5% |
| `construct/realistic_all_present/manual/prepare` | 784.1 ns | 783.1 ns – 785.3 ns | - | 782.2 ns | +0.2% |
| `construct/realistic_optional_absent/concord/input` | 47.4 ns | 47.3 ns – 47.7 ns | - | 47.3 ns | +0.3% |
| `construct/realistic_optional_absent/concord/input_prepare` | 889.4 ns | 885.5 ns – 893.4 ns | - | 1.028 µs | -13.5% |
| `construct/realistic_optional_absent/concord/prepare` | 853.2 ns | 848.6 ns – 858.1 ns | - | 1.002 µs | -14.8% |
| `construct/realistic_optional_absent/manual/input` | 41.7 ns | 41.6 ns – 41.8 ns | - | 41.4 ns | +0.6% |
| `construct/realistic_optional_absent/manual/input_prepare` | 626.9 ns | 578.6 ns – 722.6 ns | - | 677.7 ns | -7.5% |
| `construct/realistic_optional_absent/manual/prepare` | 532.1 ns | 530.0 ns – 534.4 ns | - | 540.6 ns | -1.6% |
| `construct/s2_product_scoped/concord/input` | 45.4 ns | 45.3 ns – 45.5 ns | - | 45.9 ns | -1.0% |
| `construct/s2_product_scoped/concord/input_prepare` | 767.5 ns | 764.2 ns – 771.0 ns | - | 858.4 ns | -10.6% |
| `construct/s2_product_scoped/concord/prepare` | 717.3 ns | 713.8 ns – 721.1 ns | - | 786.2 ns | -8.8% |
| `construct/s2_product_scoped/manual/input_prepare` | 521.5 ns | 520.4 ns – 522.9 ns | - | 528.8 ns | -1.4% |
| `construct/s2_product_scoped/manual/prepare` | 519.7 ns | 518.5 ns – 521.2 ns | - | 527.9 ns | -1.6% |
| `construct/s3_search_9_query/concord/input` | 55.3 ns | 55.2 ns – 55.5 ns | - | 56.8 ns | -2.6% |
| `construct/s3_search_9_query/concord/input_prepare` | 840.5 ns | 837.4 ns – 844.1 ns | - | 869.9 ns | -3.4% |
| `construct/s3_search_9_query/concord/prepare` | 836.9 ns | 834.6 ns – 840.1 ns | - | 852.8 ns | -1.9% |
| `construct/s3_search_9_query/manual/input` | 66.3 ns | 66.0 ns – 66.5 ns | - | 59.2 ns | +11.9% |
| `construct/s3_search_9_query/manual/input_prepare` | 665.5 ns | 663.1 ns – 668.3 ns | - | 662.5 ns | +0.5% |
| `construct/s3_search_9_query/manual/prepare` | 653.7 ns | 644.9 ns – 668.2 ns | - | 662.6 ns | -1.3% |
| `decode/bytes/100B/concord/decode` | 141.0 ns | 140.4 ns – 141.8 ns | 676.2 MiB/s | 140.7 ns | +0.2% |
| `decode/bytes/100B/concord/input` | 314.4 ns | 313.6 ns – 315.4 ns | 303.3 MiB/s | 315.3 ns | -0.3% |
| `decode/bytes/100B/manual/decode` | 154.2 ns | 153.2 ns – 155.2 ns | 618.6 MiB/s | 149.9 ns | +2.9% |
| `decode/bytes/100B/manual/input` | 313.4 ns | 312.7 ns – 314.2 ns | 304.3 MiB/s | 317.0 ns | -1.1% |
| `decode/bytes/10KB/concord/decode` | 142.6 ns | 141.4 ns – 143.9 ns | 68488.9 MiB/s | 141.7 ns | +0.6% |
| `decode/bytes/10KB/concord/input` | 314.8 ns | 313.9 ns – 315.9 ns | 31019.7 MiB/s | 318.0 ns | -1.0% |
| `decode/bytes/10KB/manual/decode` | 153.8 ns | 152.9 ns – 154.6 ns | 63502.6 MiB/s | 150.9 ns | +1.9% |
| `decode/bytes/10KB/manual/input` | 313.9 ns | 313.4 ns – 314.6 ns | 31106.8 MiB/s | 318.2 ns | -1.4% |
| `decode/bytes/1MB/concord/decode` | 142.6 ns | 141.3 ns – 143.8 ns | 7011691.3 MiB/s | 143.9 ns | -0.9% |
| `decode/bytes/1MB/concord/input` | 314.8 ns | 314.1 ns – 315.6 ns | 3176925.5 MiB/s | 317.4 ns | -0.8% |
| `decode/bytes/1MB/manual/decode` | 156.5 ns | 154.9 ns – 157.9 ns | 6390767.2 MiB/s | 153.3 ns | +2.1% |
| `decode/bytes/1MB/manual/input` | 315.7 ns | 314.6 ns – 317.0 ns | 3167468.5 MiB/s | 318.5 ns | -0.9% |
| `decode/json/100B/concord/decode` | 531.8 ns | 530.7 ns – 533.5 ns | 326.4 MiB/s | 551.6 ns | -3.6% |
| `decode/json/100B/concord/input` | 311.4 ns | 310.7 ns – 312.5 ns | 557.4 MiB/s | 314.7 ns | -1.0% |
| `decode/json/100B/manual/decode` | 546.0 ns | 545.0 ns – 547.3 ns | 317.9 MiB/s | 561.7 ns | -2.8% |
| `decode/json/100B/manual/input` | 311.9 ns | 311.0 ns – 313.0 ns | 556.5 MiB/s | 314.2 ns | -0.7% |
| `decode/json/10KB/concord/decode` | 21.232 µs | 21.182 µs – 21.302 µs | 463.7 MiB/s | 21.267 µs | -0.2% |
| `decode/json/10KB/concord/input` | 314.9 ns | 314.1 ns – 316.0 ns | 31259.9 MiB/s | 316.7 ns | -0.6% |
| `decode/json/10KB/manual/decode` | 21.244 µs | 21.190 µs – 21.316 µs | 463.4 MiB/s | 21.335 µs | -0.4% |
| `decode/json/10KB/manual/input` | 314.7 ns | 313.9 ns – 315.8 ns | 31279.0 MiB/s | 318.6 ns | -1.2% |
| `decode/json/1MB/concord/decode` | 2.026 ms | 2.000 ms – 2.055 ms | 493.7 MiB/s | 1.964 ms | +3.2% |
| `decode/json/1MB/concord/input` | 314.8 ns | 314.0 ns – 315.6 ns | 3177180.0 MiB/s | 318.4 ns | -1.2% |
| `decode/json/1MB/manual/decode` | 2.034 ms | 2.008 ms – 2.063 ms | 491.6 MiB/s | 1.962 ms | +3.7% |
| `decode/json/1MB/manual/input` | 314.6 ns | 313.8 ns – 315.7 ns | 3178470.2 MiB/s | 316.5 ns | -0.6% |
| `decode/text/100B/concord/decode` | 162.2 ns | 161.3 ns – 163.3 ns | 588.0 MiB/s | 159.4 ns | +1.7% |
| `decode/text/100B/concord/input` | 313.3 ns | 312.6 ns – 314.1 ns | 304.4 MiB/s | 315.9 ns | -0.8% |
| `decode/text/100B/manual/decode` | 368.3 ns | 266.3 ns – 571.3 ns | 258.9 MiB/s | 271.6 ns | +35.6% |
| `decode/text/100B/manual/input` | 314.3 ns | 313.5 ns – 315.4 ns | 303.4 MiB/s | 316.9 ns | -0.8% |
| `decode/text/10KB/concord/decode` | 594.0 ns | 576.5 ns – 611.7 ns | 16441.7 MiB/s | 646.7 ns | -8.2% |
| `decode/text/10KB/concord/input` | 315.3 ns | 314.6 ns – 316.1 ns | 30971.4 MiB/s | 318.5 ns | -1.0% |
| `decode/text/10KB/manual/decode` | 826.5 ns | 804.5 ns – 849.3 ns | 11816.2 MiB/s | 858.2 ns | -3.7% |
| `decode/text/10KB/manual/input` | 315.5 ns | 314.4 ns – 316.8 ns | 30954.9 MiB/s | 318.3 ns | -0.9% |
| `decode/text/1MB/concord/decode` | 41.490 µs | 40.701 µs – 42.304 µs | 24102.1 MiB/s | 41.531 µs | -0.1% |
| `decode/text/1MB/concord/input` | 315.6 ns | 314.8 ns – 316.6 ns | 3168617.3 MiB/s | 320.9 ns | -1.7% |
| `decode/text/1MB/manual/decode` | 44.286 µs | 44.025 µs – 44.551 µs | 22580.3 MiB/s | 44.652 µs | -0.8% |
| `decode/text/1MB/manual/input` | 314.8 ns | 314.2 ns – 315.7 ns | 3176446.3 MiB/s | 318.4 ns | -1.1% |
| `headers/1/concord_literal/prepare` | 145.1 ns | 144.8 ns – 145.6 ns | 145.1 ns/elem | - | - |
| `headers/1/concord_native/input` | 13.0 ns | 13.0 ns – 13.1 ns | 13.0 ns/elem | - | - |
| `headers/1/concord_native/prepare` | 145.6 ns | 145.3 ns – 146.0 ns | 145.6 ns/elem | - | - |
| `headers/1/concord_numeric/input` | 5.0 ns | 5.0 ns – 5.0 ns | 5.0 ns/elem | 5.0 ns | -0.8% |
| `headers/1/concord_numeric/prepare` | 169.7 ns | 169.5 ns – 170.0 ns | 169.7 ns/elem | 185.7 ns | -8.6% |
| `headers/1/concord_text/input` | 10.1 ns | 10.1 ns – 10.2 ns | 10.1 ns/elem | 10.8 ns | -6.1% |
| `headers/1/concord_text/prepare` | 158.7 ns | 158.3 ns – 159.2 ns | 158.7 ns/elem | 177.6 ns | -10.6% |
| `headers/1/manual_from_static/prepare` | 108.6 ns | 108.1 ns – 109.1 ns | 108.6 ns/elem | 108.0 ns | +0.5% |
| `headers/1/manual_from_str/input` | 10.0 ns | 10.0 ns – 10.1 ns | 10.0 ns/elem | 10.5 ns | -4.7% |
| `headers/1/manual_from_str/prepare` | 122.2 ns | 121.6 ns – 122.8 ns | 122.2 ns/elem | 121.4 ns | +0.7% |
| `headers/1/manual_numeric/input` | 0.5 ns | 0.4 ns – 0.6 ns | 0.5 ns/elem | 0.4 ns | +5.0% |
| `headers/1/manual_numeric/prepare` | 123.5 ns | 123.0 ns – 124.0 ns | 123.5 ns/elem | 124.3 ns | -0.7% |
| `headers/1/manual_prebuilt_clone/prepare` | 110.4 ns | 110.1 ns – 110.8 ns | 110.4 ns/elem | 111.0 ns | -0.5% |
| `headers/16/concord_literal/prepare` | 680.1 ns | 677.0 ns – 683.4 ns | 42.5 ns/elem | - | - |
| `headers/16/concord_native/input` | 43.4 ns | 42.3 ns – 44.7 ns | 2.7 ns/elem | - | - |
| `headers/16/concord_native/prepare` | 713.2 ns | 710.6 ns – 715.7 ns | 44.6 ns/elem | - | - |
| `headers/16/concord_numeric/input` | 11.7 ns | 11.6 ns – 11.7 ns | 0.7 ns/elem | 12.6 ns | -7.3% |
| `headers/16/concord_numeric/prepare` | 964.5 ns | 961.4 ns – 967.9 ns | 60.3 ns/elem | 1.209 µs | -20.2% |
| `headers/16/concord_text/input` | 181.6 ns | 180.9 ns – 182.2 ns | 11.3 ns/elem | 182.0 ns | -0.3% |
| `headers/16/concord_text/prepare` | 1.121 µs | 878.3 ns – 1.606 µs | 70.1 ns/elem | 1.076 µs | +4.2% |
| `headers/16/manual_from_static/prepare` | 850.9 ns | 849.0 ns – 852.9 ns | 53.2 ns/elem | 845.1 ns | +0.7% |
| `headers/16/manual_from_str/input` | 169.7 ns | 169.1 ns – 170.3 ns | 10.6 ns/elem | 171.0 ns | -0.7% |
| `headers/16/manual_from_str/prepare` | 1.135 µs | 1.132 µs – 1.138 µs | 70.9 ns/elem | 1.063 µs | +6.8% |
| `headers/16/manual_numeric/input` | 4.3 ns | 4.0 ns – 4.6 ns | 0.3 ns/elem | 7.3 ns | -41.2% |
| `headers/16/manual_numeric/prepare` | 1.081 µs | 1.079 µs – 1.084 µs | 67.6 ns/elem | 1.071 µs | +1.0% |
| `headers/16/manual_prebuilt_clone/prepare` | 878.9 ns | 877.5 ns – 880.3 ns | 54.9 ns/elem | 877.4 ns | +0.2% |
| `headers/4/concord_literal/prepare` | 219.7 ns | 219.2 ns – 220.2 ns | 54.9 ns/elem | - | - |
| `headers/4/concord_native/input` | 17.4 ns | 17.3 ns – 17.5 ns | 4.3 ns/elem | - | - |
| `headers/4/concord_native/prepare` | 235.7 ns | 234.9 ns – 236.5 ns | 58.9 ns/elem | - | - |
| `headers/4/concord_numeric/input` | 5.1 ns | 5.1 ns – 5.1 ns | 1.3 ns/elem | 5.1 ns | +0.0% |
| `headers/4/concord_numeric/prepare` | 293.5 ns | 292.9 ns – 294.3 ns | 73.4 ns/elem | 338.5 ns | -13.3% |
| `headers/4/concord_text/input` | 57.4 ns | 57.3 ns – 57.5 ns | 14.3 ns/elem | 58.2 ns | -1.5% |
| `headers/4/concord_text/prepare` | 259.6 ns | 259.2 ns – 260.1 ns | 64.9 ns/elem | 310.2 ns | -16.3% |
| `headers/4/manual_from_static/prepare` | 231.4 ns | 231.0 ns – 231.9 ns | 57.9 ns/elem | 230.5 ns | +0.4% |
| `headers/4/manual_from_str/input` | 44.7 ns | 44.5 ns – 44.9 ns | 11.2 ns/elem | 51.0 ns | -12.4% |
| `headers/4/manual_from_str/prepare` | 283.9 ns | 283.4 ns – 284.6 ns | 71.0 ns/elem | 284.7 ns | -0.3% |
| `headers/4/manual_numeric/input` | 1.9 ns | 1.5 ns – 2.3 ns | 0.5 ns/elem | 1.7 ns | +9.9% |
| `headers/4/manual_numeric/prepare` | 317.8 ns | 316.8 ns – 318.9 ns | 79.4 ns/elem | 317.6 ns | +0.0% |
| `headers/4/manual_prebuilt_clone/prepare` | 243.1 ns | 242.5 ns – 243.8 ns | 60.8 ns/elem | 242.9 ns | +0.1% |
| `roundtrip/10KB/concord/send` | 33.695 µs | 33.649 µs – 33.746 µs | 292.2 MiB/s | 33.886 µs | -0.6% |
| `roundtrip/10KB/concord/send_prepared` | 31.358 µs | 31.240 µs – 31.548 µs | 314.0 MiB/s | 30.617 µs | +2.4% |
| `roundtrip/10KB/manual/send` | 34.179 µs | 33.958 µs – 34.419 µs | 288.0 MiB/s | 33.623 µs | +1.7% |
| `roundtrip/10KB/manual/send_prepared` | 33.620 µs | 31.254 µs – 38.316 µs | 292.8 MiB/s | 31.284 µs | +7.5% |
| `roundtrip/10KB/raw_reqwest/get_bytes` | 9.518 µs | 9.516 µs – 9.521 µs | 1034.3 MiB/s | 8.708 µs | +9.3% |
| `roundtrip/small/concord/send` | 8.220 µs | 8.218 µs – 8.223 µs | 10.7 MiB/s | 8.247 µs | -0.3% |
| `roundtrip/small/concord/send_prepared` | 8.514 µs | 8.436 µs – 8.595 µs | 10.3 MiB/s | 8.194 µs | +3.9% |
| `roundtrip/small/manual/send` | 8.896 µs | 8.894 µs – 8.898 µs | 9.9 MiB/s | 8.071 µs | +10.2% |
| `roundtrip/small/manual/send_prepared` | 8.098 µs | 8.091 µs – 8.105 µs | 10.8 MiB/s | 8.122 µs | -0.3% |
| `roundtrip/small/raw_reqwest/get_bytes` | 8.096 µs | 8.093 µs – 8.100 µs | 10.8 MiB/s | 8.414 µs | -3.8% |
