# Initial public statistics

The home and network statistics pages start a request for
`https://raw.githubusercontent.com/bdlglua/BDLMinerV2/public-stats/summary.json`
while the HTML is still loading. That file contains real anonymous totals from
`https://api.bdl-pool.com:36018/api/summary`. A saved reading and its age are shown
until the live API has been checked; the snapshot never sets the Live state.
A delayed snapshot cannot replace an accepted live response.

The `Public statistics snapshot` workflow refreshes the separate `public-stats`
data branch about every five minutes. GitHub scheduling and CDN caching are
best effort. Export failures preserve the previous file, and the browser ignores
snapshots older than 30 minutes. The direct API request and its retries run
independently if the file is absent, invalid, old or unavailable. Snapshot commits
do not build or release firmware and do not redeploy the website.

The Rust `snapshot` command in `site/src/package.rs` reads only the anonymous public summary. It
checks timestamps, numbers and payload size, and includes only public summary
fields. It does not call report or private-viewer endpoints.

The browser also retains validated public readings for up to ten minutes,
respects the API's 15-second HTTP cache, and retries temporary failures with
bounded backoff. Local `?statsApi=go` previews do not load the production snapshot.

Validation:

```sh
cargo test --locked --manifest-path site/Cargo.toml
node test/web/test_site.cjs
cargo test --locked --manifest-path site/Cargo.toml
cargo run --locked --release --manifest-path site/Cargo.toml
```
