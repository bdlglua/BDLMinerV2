# Public statistics

The live public summary is served by the Supabase Edge Function:

`https://cqbvuzubjkxfsztzfzso.supabase.co/functions/v1/bdl-public-stats/api/summary`

The home page and `stats.html` query that endpoint directly. The browser also
loads the optional, static snapshot from
`https://raw.githubusercontent.com/bdlglua/BDLMinerV2/public-stats/summary.json`
while the HTML is loading. A saved reading and its age are shown until the live
endpoint has been checked; a snapshot never sets the Live state and cannot
replace an accepted live response.

The `Public statistics snapshot` workflow refreshes the separate `public-stats`
data branch from the same Supabase summary about every five minutes. GitHub
scheduling and CDN caching are best effort. Export failures preserve the previous
file, and the browser ignores snapshots older than 30 minutes. The direct API
request and its retries run independently if the file is absent, invalid, old or
unavailable. Snapshot commits do not build or release firmware and do not
redeploy the website.

The Rust `snapshot` command in `site/src/package.rs` reads only the anonymous
public summary. It checks timestamps, numbers and payload size, and includes
only public summary fields. It does not call report or private-viewer endpoints.

The browser also retains validated public readings for up to ten minutes,
respects the API's 15-second HTTP cache, and retries temporary failures with
bounded backoff. Local `?statsApi=go` previews do not load the production snapshot.

Private `My miners` telemetry uses the separate `bdl-remote-stats` Edge Function
and separate RLS-protected tables. Neither private device identifiers nor
private names enter the public summary.

Validation:

```sh
cargo test --locked --manifest-path site/Cargo.toml
node test/web/test_site.cjs
cargo test --locked --manifest-path site/Cargo.toml
cargo run --locked --release --manifest-path site/Cargo.toml
```
