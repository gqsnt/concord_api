| benchmark | mean | 95% CI (mean) | throughput | a915c8d mean | change (means) | criterion change [95% CI] |
|---|---:|---:|---:|---:|---:|---:|
| `body_encode/bytes/100B/concord/input` | 9.5 ns | 9.5 ns – 9.6 ns | 9987.9 MiB/s | 10.3 ns | -6.9% | -6.9% [-9.8%, -4.1%] |
| `body_encode/bytes/100B/concord/prepare` | 205.8 ns | 205.0 ns – 206.8 ns | 463.3 MiB/s | 184.2 ns | +11.8% | +11.8% [+11.2%, +12.4%] |
| `body_encode/bytes/100B/manual/input` | 8.2 ns | 8.2 ns – 8.3 ns | 11569.9 MiB/s | 8.2 ns | +0.3% | +0.3% [-0.7%, +1.2%] |
| `body_encode/bytes/100B/manual/prepare` | 148.3 ns | 147.6 ns – 149.1 ns | 643.2 MiB/s | 149.1 ns | -0.6% | -0.6% [-1.2%, +0.1%] |
| `body_encode/bytes/10KB/concord/input` | 9.5 ns | 9.5 ns – 9.6 ns | 1023410.4 MiB/s | 10.2 ns | -6.8% | -6.8% [-9.8%, -3.9%] |
| `body_encode/bytes/10KB/concord/prepare` | 205.6 ns | 204.9 ns – 206.4 ns | 47487.3 MiB/s | 185.7 ns | +10.8% | +10.8% [+10.3%, +11.3%] |
| `body_encode/bytes/10KB/manual/input` | 8.2 ns | 8.2 ns – 8.2 ns | 1190538.0 MiB/s | 8.3 ns | -1.2% | -1.2% [-2.3%, -0.3%] |
| `body_encode/bytes/10KB/manual/prepare` | 147.6 ns | 147.0 ns – 148.2 ns | 66167.4 MiB/s | 149.4 ns | -1.2% | -1.2% [-1.7%, -0.7%] |
| `body_encode/bytes/1MB/concord/input` | 9.5 ns | 9.5 ns – 9.6 ns | 105034326.5 MiB/s | 10.3 ns | -7.2% | -7.2% [-10.1%, -4.3%] |
| `body_encode/bytes/1MB/concord/prepare` | 205.3 ns | 204.3 ns – 206.1 ns | 4871267.5 MiB/s | 190.3 ns | +7.9% | +7.9% [+7.0%, +8.6%] |
| `body_encode/bytes/1MB/manual/input` | 8.2 ns | 8.2 ns – 8.3 ns | 121594418.1 MiB/s | 8.2 ns | +0.1% | +0.1% [-0.5%, +0.9%] |
| `body_encode/bytes/1MB/manual/prepare` | 148.5 ns | 147.8 ns – 149.2 ns | 6732858.5 MiB/s | 151.5 ns | -2.0% | -2.0% [-2.7%, -1.2%] |
| `body_encode/json/100B/concord/input` | 88.8 ns | 88.3 ns – 89.2 ns | 1955.4 MiB/s | 89.1 ns | -0.4% | -0.4% [-1.1%, +0.3%] |
| `body_encode/json/100B/concord/prepare` | 499.0 ns | 498.2 ns – 500.0 ns | 347.8 MiB/s | 481.4 ns | +3.7% | +3.7% [+3.4%, +3.9%] |
| `body_encode/json/100B/manual/input` | 88.5 ns | 88.0 ns – 89.1 ns | 1961.6 MiB/s | 89.7 ns | -1.3% | -1.3% [-2.1%, -0.4%] |
| `body_encode/json/100B/manual/prepare` | 429.0 ns | 428.2 ns – 429.9 ns | 404.5 MiB/s | 430.5 ns | -0.3% | -0.3% [-0.6%, -0.1%] |
| `body_encode/json/10KB/concord/input` | 4.451 µs | 4.442 µs – 4.462 µs | 2211.7 MiB/s | 4.509 µs | -1.3% | -1.3% [-1.7%, -0.9%] |
| `body_encode/json/10KB/concord/prepare` | 11.443 µs | 11.432 µs – 11.456 µs | 860.3 MiB/s | 11.998 µs | -4.6% | -4.6% [-4.8%, -4.5%] |
| `body_encode/json/10KB/manual/input` | 4.484 µs | 4.469 µs – 4.501 µs | 2195.7 MiB/s | 4.504 µs | -0.4% | -0.4% [-1.0%, +0.1%] |
| `body_encode/json/10KB/manual/prepare` | 11.550 µs | 11.537 µs – 11.564 µs | 852.4 MiB/s | 11.338 µs | +1.9% | +1.9% [+1.7%, +2.0%] |
| `body_encode/json/1MB/concord/input` | 421.253 µs | 420.236 µs – 422.268 µs | 2374.0 MiB/s | 421.867 µs | -0.1% | -0.1% [-0.5%, +0.2%] |
| `body_encode/json/1MB/concord/prepare` | 1.039 ms | 1.038 ms – 1.040 ms | 962.8 MiB/s | 1.127 ms | -7.8% | -7.8% [-8.0%, -7.6%] |
| `body_encode/json/1MB/manual/input` | 426.891 µs | 426.057 µs – 427.760 µs | 2342.6 MiB/s | 422.100 µs | +1.1% | +1.1% [+0.8%, +1.5%] |
| `body_encode/json/1MB/manual/prepare` | 1.069 ms | 1.068 ms – 1.070 ms | 935.4 MiB/s | 1.035 ms | +3.3% | +3.3% [+3.1%, +3.5%] |
| `body_encode/text/100B/concord/input` | 11.6 ns | 11.5 ns – 11.7 ns | 8232.7 MiB/s | 11.9 ns | -2.5% | -2.5% [-3.2%, -1.7%] |
| `body_encode/text/100B/concord/prepare` | 201.2 ns | 200.6 ns – 202.1 ns | 473.9 MiB/s | 202.6 ns | -0.7% | -0.7% [-1.2%, -0.1%] |
| `body_encode/text/100B/manual/input` | 10.8 ns | 10.7 ns – 10.9 ns | 8815.3 MiB/s | 10.8 ns | +0.3% | +0.3% [-0.6%, +1.2%] |
| `body_encode/text/100B/manual/prepare` | 151.7 ns | 151.1 ns – 152.5 ns | 628.5 MiB/s | 152.7 ns | -0.6% | -0.6% [-1.2%, -0.1%] |
| `body_encode/text/10KB/concord/input` | 433.4 ns | 402.4 ns – 464.6 ns | 22531.0 MiB/s | 441.0 ns | -1.7% | -1.7% [-11.5%, +8.7%] |
| `body_encode/text/10KB/concord/prepare` | 209.0 ns | 207.5 ns – 210.7 ns | 46726.0 MiB/s | 210.3 ns | -0.6% | -0.6% [-1.8%, +0.5%] |
| `body_encode/text/10KB/manual/input` | 403.7 ns | 376.8 ns – 430.6 ns | 24189.6 MiB/s | 399.1 ns | +1.1% | +1.1% [-8.0%, +10.8%] |
| `body_encode/text/10KB/manual/prepare` | 160.4 ns | 159.0 ns – 162.2 ns | 60884.8 MiB/s | 160.4 ns | +0.0% | +0.0% [-1.3%, +1.4%] |
| `body_encode/text/1MB/concord/input` | 41.122 µs | 38.835 µs – 43.465 µs | 24317.9 MiB/s | 42.271 µs | -2.7% | -2.7% [-10.3%, +5.2%] |
| `body_encode/text/1MB/concord/prepare` | 279.1 ns | 273.8 ns – 284.7 ns | 3583399.9 MiB/s | 283.6 ns | -1.6% | -1.6% [-3.6%, +0.3%] |
| `body_encode/text/1MB/manual/input` | 40.147 µs | 38.509 µs – 41.769 µs | 24908.2 MiB/s | 38.924 µs | +3.1% | +3.1% [-3.0%, +9.8%] |
| `body_encode/text/1MB/manual/prepare` | 253.3 ns | 250.6 ns – 255.9 ns | 3948228.3 MiB/s | 245.8 ns | +3.0% | +3.0% [+1.6%, +4.6%] |
| `compose/append/concord/input` | 9.4 ns | 9.4 ns – 9.4 ns | - | 9.4 ns | -0.6% | -0.6% [-1.4%, -0.0%] |
| `compose/append/concord/prepare` | 449.7 ns | 447.4 ns – 452.9 ns | - | 533.0 ns | -15.6% | -15.6% [-16.3%, -14.9%] |
| `compose/append/manual/input` | 10.2 ns | 10.1 ns – 10.3 ns | - | 10.2 ns | +0.0% | +0.0% [-0.9%, +1.1%] |
| `compose/append/manual/prepare` | 387.4 ns | 386.6 ns – 388.4 ns | - | 386.9 ns | +0.1% | +0.1% [-0.1%, +0.4%] |
| `compose/deep8_navigate/concord/input` | 295.9 ns | 295.4 ns – 296.6 ns | - | 300.2 ns | -1.4% | -1.4% [-1.7%, -1.1%] |
| `compose/deep8_navigate/concord/input_prepare` | 1.781 µs | 1.778 µs – 1.785 µs | - | 1.918 µs | -7.1% | -7.1% [-7.4%, -6.9%] |
| `compose/deep8_navigate/concord/prepare` | 1.562 µs | 1.556 µs – 1.570 µs | - | 1.696 µs | -7.9% | -7.9% [-8.5%, -7.3%] |
| `compose/deep8_navigate/manual/input` | 190.3 ns | 189.9 ns – 190.8 ns | - | 186.8 ns | +1.9% | +1.9% [+1.5%, +2.2%] |
| `compose/deep8_navigate/manual/input_prepare` | 1.203 µs | 1.202 µs – 1.206 µs | - | 1.182 µs | +1.8% | +1.8% [+1.5%, +2.1%] |
| `compose/deep8_navigate/manual/prepare` | 1.010 µs | 1.007 µs – 1.012 µs | - | 1.006 µs | +0.4% | +0.4% [+0.1%, +0.7%] |
| `compose/deep8_reused_scope/concord/input` | 5.8 ns | 5.8 ns – 5.8 ns | - | 5.8 ns | -0.1% | -0.1% [-0.4%, +0.1%] |
| `compose/deep8_reused_scope/concord/input_prepare` | 1.357 µs | 1.355 µs – 1.361 µs | - | 1.487 µs | -8.8% | -8.8% [-9.1%, -8.4%] |
| `compose/deep8_reused_scope/concord/prepare` | 1.347 µs | 1.345 µs – 1.350 µs | - | 1.492 µs | -9.7% | -9.7% [-10.2%, -9.3%] |
| `compose/deep8_reused_scope/manual/input_prepare` | 481.7 ns | 479.7 ns – 483.9 ns | - | 487.0 ns | -1.1% | -1.1% [-1.6%, -0.6%] |
| `compose/deep8_reused_scope/manual/prepare` | 482.3 ns | 480.2 ns – 484.6 ns | - | 485.7 ns | -0.7% | -0.7% [-1.2%, -0.1%] |
| `compose/inherit/concord/input` | 5.8 ns | 5.7 ns – 5.8 ns | - | 5.8 ns | -0.5% | -0.5% [-0.7%, -0.3%] |
| `compose/inherit/concord/prepare` | 406.8 ns | 405.6 ns – 408.5 ns | - | 481.3 ns | -15.5% | -15.5% [-15.8%, -15.1%] |
| `compose/inherit/manual/input` | 0.8 ns | 0.8 ns – 0.9 ns | - | 1.0 ns | -20.7% | -20.7% [-35.6%, -3.2%] |
| `compose/inherit/manual/prepare` | 338.9 ns | 338.4 ns – 339.4 ns | - | 337.2 ns | +0.5% | +0.5% [+0.2%, +0.7%] |
| `compose/override_absent/concord/input` | 5.2 ns | 5.2 ns – 5.3 ns | - | 5.2 ns | +1.0% | +1.0% [-0.7%, +2.7%] |
| `compose/override_absent/concord/prepare` | 411.7 ns | 410.8 ns – 412.6 ns | - | 485.2 ns | -15.2% | -15.2% [-15.5%, -14.9%] |
| `compose/override_absent/manual/input` | 0.8 ns | 0.7 ns – 0.8 ns | - | 0.9 ns | -12.4% | -12.4% [-26.8%, +3.0%] |
| `compose/override_absent/manual/prepare` | 339.4 ns | 338.9 ns – 340.1 ns | - | 338.0 ns | +0.4% | +0.4% [+0.1%, +0.7%] |
| `compose/override_present/concord/input` | 13.1 ns | 13.0 ns – 13.1 ns | - | 13.0 ns | +0.2% | +0.2% [-0.1%, +0.5%] |
| `compose/override_present/concord/prepare` | 410.5 ns | 409.1 ns – 412.3 ns | - | 482.6 ns | -14.9% | -14.9% [-15.3%, -14.5%] |
| `compose/override_present/manual/input` | 10.1 ns | 10.0 ns – 10.1 ns | - | 10.1 ns | -0.5% | -0.5% [-0.9%, -0.0%] |
| `compose/override_present/manual/prepare` | 346.6 ns | 345.9 ns – 347.4 ns | - | 344.7 ns | +0.5% | +0.5% [+0.2%, +0.9%] |
| `compose/remove/concord/input` | 5.8 ns | 5.8 ns – 5.8 ns | - | 5.8 ns | -0.0% | -0.0% [-0.3%, +0.2%] |
| `compose/remove/concord/prepare` | 375.6 ns | 374.6 ns – 376.7 ns | - | 440.0 ns | -14.6% | -14.6% [-15.1%, -14.3%] |
| `compose/remove/manual/input` | 0.8 ns | 0.8 ns – 0.9 ns | - | 1.0 ns | -17.4% | -17.4% [-32.6%, +0.1%] |
| `compose/remove/manual/prepare` | 295.6 ns | 295.0 ns – 296.3 ns | - | 294.6 ns | +0.3% | +0.3% [+0.1%, +0.6%] |
| `construct/minimal/concord/input` | 5.9 ns | 5.9 ns – 5.9 ns | - | 5.9 ns | +0.4% | +0.4% [+0.2%, +0.5%] |
| `construct/minimal/concord/input_prepare` | 144.0 ns | 143.5 ns – 144.6 ns | - | 142.3 ns | +1.2% | +1.2% [+0.8%, +1.7%] |
| `construct/minimal/concord/prepare` | 145.3 ns | 144.8 ns – 145.9 ns | - | 138.5 ns | +4.9% | +4.9% [+4.4%, +5.4%] |
| `construct/minimal/manual/input_prepare` | 101.9 ns | 101.6 ns – 102.1 ns | - | 102.5 ns | -0.6% | -0.6% [-1.0%, -0.2%] |
| `construct/minimal/manual/prepare` | 102.8 ns | 102.3 ns – 103.4 ns | - | 102.3 ns | +0.5% | +0.5% [-0.2%, +1.2%] |
| `construct/realistic_all_present/concord/input` | 111.0 ns | 109.5 ns – 112.6 ns | - | 104.9 ns | +5.8% | +5.8% [+4.2%, +7.3%] |
| `construct/realistic_all_present/concord/input_prepare` | 1.478 µs | 1.475 µs – 1.482 µs | - | 1.529 µs | -3.3% | -3.3% [-3.7%, -2.9%] |
| `construct/realistic_all_present/concord/prepare` | 1.414 µs | 1.409 µs – 1.420 µs | - | 1.550 µs | -8.8% | -8.8% [-9.3%, -8.3%] |
| `construct/realistic_all_present/manual/input` | 92.1 ns | 91.8 ns – 92.5 ns | - | 91.4 ns | +0.8% | +0.8% [+0.3%, +1.3%] |
| `construct/realistic_all_present/manual/input_prepare` | 826.4 ns | 823.7 ns – 829.5 ns | - | 826.9 ns | -0.1% | -0.1% [-0.5%, +0.4%] |
| `construct/realistic_all_present/manual/prepare` | 790.5 ns | 789.6 ns – 791.8 ns | - | 782.2 ns | +1.1% | +1.1% [+0.8%, +1.3%] |
| `construct/realistic_optional_absent/concord/input` | 48.0 ns | 47.8 ns – 48.2 ns | - | 47.3 ns | +1.5% | +1.5% [+1.0%, +2.0%] |
| `construct/realistic_optional_absent/concord/input_prepare` | 909.4 ns | 906.8 ns – 912.3 ns | - | 1.028 µs | -11.5% | -11.5% [-11.9%, -11.2%] |
| `construct/realistic_optional_absent/concord/prepare` | 900.8 ns | 860.6 ns – 977.9 ns | - | 1.002 µs | -10.1% | -10.1% [-14.2%, -2.3%] |
| `construct/realistic_optional_absent/manual/input` | 41.5 ns | 41.3 ns – 41.6 ns | - | 41.4 ns | +0.0% | +0.0% [-0.3%, +0.4%] |
| `construct/realistic_optional_absent/manual/input_prepare` | 631.2 ns | 580.9 ns – 731.0 ns | - | 677.7 ns | -6.9% | -6.9% [-28.6%, +19.8%] |
| `construct/realistic_optional_absent/manual/prepare` | 539.0 ns | 536.6 ns – 541.7 ns | - | 540.6 ns | -0.3% | -0.3% [-0.8%, +0.2%] |
| `construct/s2_product_scoped/concord/input` | 45.5 ns | 45.5 ns – 45.6 ns | - | 45.9 ns | -0.7% | -0.7% [-0.9%, -0.5%] |
| `construct/s2_product_scoped/concord/input_prepare` | 783.8 ns | 781.6 ns – 786.3 ns | - | 858.4 ns | -8.7% | -8.7% [-9.0%, -8.3%] |
| `construct/s2_product_scoped/concord/prepare` | 731.0 ns | 727.9 ns – 734.3 ns | - | 786.2 ns | -7.0% | -7.0% [-7.5%, -6.6%] |
| `construct/s2_product_scoped/manual/input_prepare` | 526.7 ns | 525.9 ns – 527.5 ns | - | 528.8 ns | -0.4% | -0.4% [-0.8%, -0.1%] |
| `construct/s2_product_scoped/manual/prepare` | 526.8 ns | 525.7 ns – 528.2 ns | - | 527.9 ns | -0.2% | -0.2% [-0.6%, +0.2%] |
| `construct/s3_search_9_query/concord/input` | 56.2 ns | 55.9 ns – 56.4 ns | - | 56.8 ns | -1.1% | -1.1% [-1.7%, -0.5%] |
| `construct/s3_search_9_query/concord/input_prepare` | 862.8 ns | 855.2 ns – 875.1 ns | - | 869.9 ns | -0.8% | -0.8% [-1.8%, +0.7%] |
| `construct/s3_search_9_query/concord/prepare` | 856.7 ns | 854.8 ns – 859.0 ns | - | 852.8 ns | +0.5% | +0.5% [+0.1%, +0.8%] |
| `construct/s3_search_9_query/manual/input` | 66.0 ns | 65.7 ns – 66.3 ns | - | 59.2 ns | +11.5% | +11.5% [+10.7%, +12.3%] |
| `construct/s3_search_9_query/manual/input_prepare` | 659.7 ns | 658.0 ns – 661.5 ns | - | 662.5 ns | -0.4% | -0.4% [-0.8%, -0.0%] |
| `construct/s3_search_9_query/manual/prepare` | 658.4 ns | 651.4 ns – 668.6 ns | - | 662.6 ns | -0.6% | -0.6% [-1.8%, +1.1%] |
| `decode/bytes/100B/concord/decode` | 143.3 ns | 142.7 ns – 144.0 ns | 665.6 MiB/s | 140.7 ns | +1.8% | +1.8% [+1.2%, +2.5%] |
| `decode/bytes/100B/concord/input` | 320.9 ns | 320.0 ns – 321.9 ns | 297.2 MiB/s | 315.3 ns | +1.7% | +1.7% [+1.3%, +2.2%] |
| `decode/bytes/100B/manual/decode` | 151.1 ns | 150.6 ns – 151.8 ns | 631.0 MiB/s | 149.9 ns | +0.8% | +0.8% [+0.3%, +1.4%] |
| `decode/bytes/100B/manual/input` | 320.3 ns | 319.2 ns – 321.7 ns | 297.8 MiB/s | 317.0 ns | +1.0% | +1.0% [+0.4%, +1.7%] |
| `decode/bytes/10KB/concord/decode` | 142.8 ns | 142.2 ns – 143.7 ns | 68365.2 MiB/s | 141.7 ns | +0.8% | +0.8% [+0.1%, +1.6%] |
| `decode/bytes/10KB/concord/input` | 323.1 ns | 322.0 ns – 324.2 ns | 30225.7 MiB/s | 318.0 ns | +1.6% | +1.6% [+1.2%, +2.0%] |
| `decode/bytes/10KB/manual/decode` | 151.2 ns | 150.6 ns – 151.9 ns | 64582.2 MiB/s | 150.9 ns | +0.2% | +0.2% [-0.4%, +0.9%] |
| `decode/bytes/10KB/manual/input` | 322.2 ns | 321.3 ns – 323.2 ns | 30311.5 MiB/s | 318.2 ns | +1.2% | +1.2% [+0.8%, +1.6%] |
| `decode/bytes/1MB/concord/decode` | 144.1 ns | 142.7 ns – 145.4 ns | 6939151.2 MiB/s | 143.9 ns | +0.1% | +0.1% [-1.2%, +1.4%] |
| `decode/bytes/1MB/concord/input` | 322.7 ns | 321.7 ns – 323.9 ns | 3098559.0 MiB/s | 317.4 ns | +1.7% | +1.7% [+1.2%, +2.2%] |
| `decode/bytes/1MB/manual/decode` | 152.0 ns | 150.7 ns – 153.1 ns | 6578416.0 MiB/s | 153.3 ns | -0.8% | -0.8% [-1.9%, +0.4%] |
| `decode/bytes/1MB/manual/input` | 323.0 ns | 321.9 ns – 324.5 ns | 3095900.1 MiB/s | 318.5 ns | +1.4% | +1.4% [+0.9%, +2.0%] |
| `decode/json/100B/concord/decode` | 543.0 ns | 542.0 ns – 544.6 ns | 319.6 MiB/s | 551.6 ns | -1.5% | -1.5% [-1.8%, -1.2%] |
| `decode/json/100B/concord/input` | 317.6 ns | 316.7 ns – 318.6 ns | 546.4 MiB/s | 314.7 ns | +0.9% | +0.9% [+0.5%, +1.4%] |
| `decode/json/100B/manual/decode` | 537.4 ns | 536.3 ns – 538.8 ns | 323.0 MiB/s | 561.7 ns | -4.3% | -4.3% [-4.7%, -4.0%] |
| `decode/json/100B/manual/input` | 317.5 ns | 316.6 ns – 318.6 ns | 546.7 MiB/s | 314.2 ns | +1.0% | +1.0% [+0.6%, +1.5%] |
| `decode/json/10KB/concord/decode` | 21.342 µs | 21.291 µs – 21.414 µs | 461.3 MiB/s | 21.267 µs | +0.4% | +0.4% [-0.0%, +0.8%] |
| `decode/json/10KB/concord/input` | 320.9 ns | 319.8 ns – 322.2 ns | 30673.9 MiB/s | 316.7 ns | +1.3% | +1.3% [+0.8%, +1.8%] |
| `decode/json/10KB/manual/decode` | 21.343 µs | 21.292 µs – 21.412 µs | 461.3 MiB/s | 21.335 µs | +0.0% | +0.0% [-0.6%, +0.5%] |
| `decode/json/10KB/manual/input` | 319.9 ns | 319.0 ns – 321.1 ns | 30772.4 MiB/s | 318.6 ns | +0.4% | +0.4% [-0.1%, +0.9%] |
| `decode/json/1MB/concord/decode` | 1.970 ms | 1.968 ms – 1.971 ms | 507.7 MiB/s | 1.964 ms | +0.3% | +0.3% [+0.1%, +0.5%] |
| `decode/json/1MB/concord/input` | 319.9 ns | 319.0 ns – 320.9 ns | 3126304.3 MiB/s | 318.4 ns | +0.5% | +0.5% [-0.0%, +0.9%] |
| `decode/json/1MB/manual/decode` | 1.968 ms | 1.967 ms – 1.970 ms | 508.1 MiB/s | 1.962 ms | +0.3% | +0.3% [+0.1%, +0.5%] |
| `decode/json/1MB/manual/input` | 320.8 ns | 319.8 ns – 321.9 ns | 3117787.3 MiB/s | 316.5 ns | +1.3% | +1.3% [+0.9%, +1.8%] |
| `decode/text/100B/concord/decode` | 160.5 ns | 159.4 ns – 161.8 ns | 594.2 MiB/s | 159.4 ns | +0.7% | +0.7% [-0.3%, +1.7%] |
| `decode/text/100B/concord/input` | 321.5 ns | 320.5 ns – 322.6 ns | 296.6 MiB/s | 315.9 ns | +1.8% | +1.8% [+1.4%, +2.2%] |
| `decode/text/100B/manual/decode` | 375.0 ns | 262.1 ns – 599.6 ns | 254.3 MiB/s | 271.6 ns | +38.1% | +38.1% [-3.8%, +121.1%] |
| `decode/text/100B/manual/input` | 321.7 ns | 320.7 ns – 322.9 ns | 296.4 MiB/s | 316.9 ns | +1.5% | +1.5% [+1.1%, +2.0%] |
| `decode/text/10KB/concord/decode` | 615.2 ns | 597.1 ns – 633.2 ns | 15874.8 MiB/s | 646.7 ns | -4.9% | -4.9% [-9.3%, -0.5%] |
| `decode/text/10KB/concord/input` | 324.1 ns | 323.1 ns – 325.1 ns | 30135.4 MiB/s | 318.5 ns | +1.7% | +1.7% [+1.2%, +2.3%] |
| `decode/text/10KB/manual/decode` | 779.8 ns | 763.1 ns – 797.2 ns | 12522.7 MiB/s | 858.2 ns | -9.1% | -9.1% [-12.0%, -6.0%] |
| `decode/text/10KB/manual/input` | 324.0 ns | 322.9 ns – 325.3 ns | 30138.0 MiB/s | 318.3 ns | +1.8% | +1.8% [+1.4%, +2.3%] |
| `decode/text/1MB/concord/decode` | 42.745 µs | 41.971 µs – 43.555 µs | 23394.5 MiB/s | 41.531 µs | +2.9% | +2.9% [+0.2%, +5.8%] |
| `decode/text/1MB/concord/input` | 324.1 ns | 323.0 ns – 325.4 ns | 3085382.1 MiB/s | 320.9 ns | +1.0% | +1.0% [+0.4%, +1.5%] |
| `decode/text/1MB/manual/decode` | 44.324 µs | 44.040 µs – 44.612 µs | 22561.3 MiB/s | 44.652 µs | -0.7% | -0.7% [-1.7%, +0.3%] |
| `decode/text/1MB/manual/input` | 324.2 ns | 323.2 ns – 325.2 ns | 3084689.9 MiB/s | 318.4 ns | +1.8% | +1.8% [+1.4%, +2.2%] |
| `headers/1/concord_literal/prepare` | 146.3 ns | 145.8 ns – 146.8 ns | 146.3 ns/elem | - | - | - |
| `headers/1/concord_native/input` | 13.1 ns | 13.0 ns – 13.1 ns | 13.1 ns/elem | - | - | - |
| `headers/1/concord_native/prepare` | 150.8 ns | 150.4 ns – 151.3 ns | 150.8 ns/elem | - | - | - |
| `headers/1/concord_numeric/input` | 5.0 ns | 5.0 ns – 5.0 ns | 5.0 ns/elem | 5.0 ns | -0.5% | -0.5% [-0.7%, -0.2%] |
| `headers/1/concord_numeric/prepare` | 176.2 ns | 175.9 ns – 176.5 ns | 176.2 ns/elem | 185.7 ns | -5.2% | -5.2% [-5.5%, -4.9%] |
| `headers/1/concord_text/input` | 10.2 ns | 10.2 ns – 10.3 ns | 10.2 ns/elem | 10.8 ns | -5.3% | -5.3% [-6.5%, -4.2%] |
| `headers/1/concord_text/prepare` | 159.6 ns | 159.2 ns – 160.3 ns | 159.6 ns/elem | 177.6 ns | -10.1% | -10.1% [-10.4%, -9.7%] |
| `headers/1/manual_from_static/prepare` | 107.7 ns | 107.3 ns – 108.2 ns | 107.7 ns/elem | 108.0 ns | -0.3% | -0.3% [-1.1%, +0.4%] |
| `headers/1/manual_from_str/input` | 10.0 ns | 10.0 ns – 10.0 ns | 10.0 ns/elem | 10.5 ns | -4.8% | -4.8% [-5.4%, -4.2%] |
| `headers/1/manual_from_str/prepare` | 123.8 ns | 123.2 ns – 124.5 ns | 123.8 ns/elem | 121.4 ns | +2.0% | +2.0% [+1.5%, +2.6%] |
| `headers/1/manual_numeric/input` | 0.4 ns | 0.3 ns – 0.5 ns | 0.4 ns/elem | 0.4 ns | -4.3% | -4.3% [-30.4%, +34.6%] |
| `headers/1/manual_numeric/prepare` | 121.9 ns | 121.6 ns – 122.4 ns | 121.9 ns/elem | 124.3 ns | -1.9% | -1.9% [-2.4%, -1.3%] |
| `headers/1/manual_prebuilt_clone/prepare` | 110.8 ns | 110.3 ns – 111.4 ns | 110.8 ns/elem | 111.0 ns | -0.1% | -0.1% [-0.8%, +0.5%] |
| `headers/16/concord_literal/prepare` | 678.8 ns | 677.5 ns – 680.3 ns | 42.4 ns/elem | - | - | - |
| `headers/16/concord_native/input` | 41.6 ns | 40.8 ns – 42.4 ns | 2.6 ns/elem | - | - | - |
| `headers/16/concord_native/prepare` | 702.4 ns | 701.3 ns – 703.6 ns | 43.9 ns/elem | - | - | - |
| `headers/16/concord_numeric/input` | 12.1 ns | 12.1 ns – 12.2 ns | 0.8 ns/elem | 12.6 ns | -3.4% | -3.4% [-6.3%, -0.5%] |
| `headers/16/concord_numeric/prepare` | 960.1 ns | 956.6 ns – 964.3 ns | 60.0 ns/elem | 1.209 µs | -20.6% | -20.6% [-21.0%, -20.1%] |
| `headers/16/concord_text/input` | 182.9 ns | 182.4 ns – 183.5 ns | 11.4 ns/elem | 182.0 ns | +0.5% | +0.5% [+0.1%, +0.9%] |
| `headers/16/concord_text/prepare` | 879.3 ns | 876.2 ns – 882.7 ns | 55.0 ns/elem | 1.076 µs | -18.3% | -18.3% [-18.6%, -18.0%] |
| `headers/16/manual_from_static/prepare` | 848.0 ns | 846.3 ns – 849.7 ns | 53.0 ns/elem | 845.1 ns | +0.3% | +0.3% [+0.0%, +0.7%] |
| `headers/16/manual_from_str/input` | 171.7 ns | 171.2 ns – 172.4 ns | 10.7 ns/elem | 171.0 ns | +0.4% | +0.4% [-0.0%, +0.9%] |
| `headers/16/manual_from_str/prepare` | 1.083 µs | 1.078 µs – 1.089 µs | 67.7 ns/elem | 1.063 µs | +1.9% | +1.9% [+1.4%, +2.5%] |
| `headers/16/manual_numeric/input` | 6.8 ns | 5.3 ns – 8.5 ns | 0.4 ns/elem | 7.3 ns | -6.4% | -6.4% [-31.4%, +28.3%] |
| `headers/16/manual_numeric/prepare` | 1.079 µs | 1.076 µs – 1.082 µs | 67.4 ns/elem | 1.071 µs | +0.7% | +0.7% [+0.4%, +1.2%] |
| `headers/16/manual_prebuilt_clone/prepare` | 879.2 ns | 877.9 ns – 880.7 ns | 55.0 ns/elem | 877.4 ns | +0.2% | +0.2% [-0.1%, +0.5%] |
| `headers/4/concord_literal/prepare` | 224.2 ns | 223.6 ns – 224.7 ns | 56.0 ns/elem | - | - | - |
| `headers/4/concord_native/input` | 17.4 ns | 17.4 ns – 17.5 ns | 4.4 ns/elem | - | - | - |
| `headers/4/concord_native/prepare` | 236.5 ns | 235.8 ns – 237.3 ns | 59.1 ns/elem | - | - | - |
| `headers/4/concord_numeric/input` | 5.1 ns | 5.1 ns – 5.1 ns | 1.3 ns/elem | 5.1 ns | +0.2% | +0.2% [-0.2%, +0.6%] |
| `headers/4/concord_numeric/prepare` | 292.5 ns | 290.6 ns – 295.7 ns | 73.1 ns/elem | 338.5 ns | -13.6% | -13.6% [-14.2%, -12.5%] |
| `headers/4/concord_text/input` | 58.2 ns | 57.9 ns – 58.6 ns | 14.6 ns/elem | 58.2 ns | -0.0% | -0.0% [-0.9%, +0.8%] |
| `headers/4/concord_text/prepare` | 256.4 ns | 255.9 ns – 257.1 ns | 64.1 ns/elem | 310.2 ns | -17.3% | -17.3% [-17.6%, -17.1%] |
| `headers/4/manual_from_static/prepare` | 230.9 ns | 230.4 ns – 231.4 ns | 57.7 ns/elem | 230.5 ns | +0.2% | +0.2% [-0.1%, +0.5%] |
| `headers/4/manual_from_str/input` | 44.9 ns | 44.8 ns – 45.0 ns | 11.2 ns/elem | 51.0 ns | -12.0% | -12.0% [-13.1%, -10.9%] |
| `headers/4/manual_from_str/prepare` | 288.3 ns | 284.8 ns – 293.0 ns | 72.1 ns/elem | 284.7 ns | +1.3% | +1.3% [+0.1%, +2.9%] |
| `headers/4/manual_numeric/input` | 1.7 ns | 1.3 ns – 2.1 ns | 0.4 ns/elem | 1.7 ns | -0.7% | -0.7% [-27.1%, +37.7%] |
| `headers/4/manual_numeric/prepare` | 321.4 ns | 320.3 ns – 322.6 ns | 80.4 ns/elem | 317.6 ns | +1.2% | +1.2% [+0.7%, +1.7%] |
| `headers/4/manual_prebuilt_clone/prepare` | 243.5 ns | 242.7 ns – 244.9 ns | 60.9 ns/elem | 242.9 ns | +0.3% | +0.3% [-0.2%, +0.9%] |
| `roundtrip/10KB/concord/send` | 34.607 µs | 34.541 µs – 34.668 µs | 284.5 MiB/s | 33.886 µs | +2.1% | +2.1% [+1.9%, +2.3%] |
| `roundtrip/10KB/concord/send_prepared` | 31.302 µs | 30.492 µs – 32.845 µs | 314.5 MiB/s | 30.617 µs | +2.2% | +2.2% [-0.4%, +7.2%] |
| `roundtrip/10KB/manual/send` | 33.507 µs | 33.491 µs – 33.524 µs | 293.8 MiB/s | 33.623 µs | -0.3% | -0.3% [-0.5%, -0.2%] |
| `roundtrip/10KB/manual/send_prepared` | 33.414 µs | 30.356 µs – 39.482 µs | 294.6 MiB/s | 31.284 µs | +6.8% | +6.8% [-3.1%, +26.1%] |
| `roundtrip/10KB/raw_reqwest/get_bytes` | 8.797 µs | 8.769 µs – 8.831 µs | 1119.1 MiB/s | 8.708 µs | +1.0% | +1.0% [+0.7%, +1.4%] |
| `roundtrip/small/concord/send` | 8.511 µs | 8.438 µs – 8.586 µs | 10.3 MiB/s | 8.247 µs | +3.2% | +3.2% [+2.3%, +4.1%] |
| `roundtrip/small/concord/send_prepared` | 8.179 µs | 8.171 µs – 8.186 µs | 10.7 MiB/s | 8.194 µs | -0.2% | -0.2% [-0.3%, -0.0%] |
| `roundtrip/small/manual/send` | 8.897 µs | 8.893 µs – 8.902 µs | 9.9 MiB/s | 8.071 µs | +10.2% | +10.2% [+10.2%, +10.3%] |
| `roundtrip/small/manual/send_prepared` | 8.114 µs | 8.106 µs – 8.121 µs | 10.8 MiB/s | 8.122 µs | -0.1% | -0.1% [-0.2%, +0.0%] |
| `roundtrip/small/raw_reqwest/get_bytes` | 8.162 µs | 8.156 µs – 8.170 µs | 10.7 MiB/s | 8.414 µs | -3.0% | -3.0% [-3.9%, -2.1%] |
