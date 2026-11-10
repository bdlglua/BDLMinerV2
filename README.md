# Public BDLMiner statistics

`summary.json` is a bounded copy of the anonymous public summary at
https://api.bdl-pool.com:36018/api/summary. Its `at` value is the measurement
 timestamp in Unix milliseconds.

The website displays this reading with its age while it connects to the live
API. The Public statistics snapshot workflow updates this branch approximately
every five minutes; GitHub scheduling and CDN caches may delay an update.
