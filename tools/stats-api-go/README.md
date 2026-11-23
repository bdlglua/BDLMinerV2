# Local Go statistics API test fixture

This is a local, read-only test fixture for the public website's
`GET /api/summary` response contract. It returns clearly synthetic sample data;
it does not collect miner reports, persist data or provide a production API.

The production statistics backend is hosted entirely on Supabase. The public
website and current firmware use separate Supabase Edge Functions for anonymous
public statistics and optional private **My miners** telemetry. This Go fixture
is not part of the production request path and does not replace or proxy
Supabase.

## Run locally

Run it from the prototype directory:

```sh
cd tools/stats-api-go
go run .
```

The server binds to `127.0.0.1:8787` by default. Its `/api/summary` response
uses the fields consumed by the public statistics page, and `/healthz` reports
whether the fixture is running. A different listen address may be provided
with `-addr`, but non-loopback addresses are refused.

To see the synthetic data in the packaged site, open the local `stats.html`
preview with `?statsApi=go` appended to its URL, for example
`file:///path/to/stats.html?statsApi=go`. The override is honored only for
`file:` pages and pages served from localhost; published site URLs always use
the production Supabase endpoint.

## Tests

```sh
cd tools/stats-api-go
go test ./...
```

This fixture intentionally omits report ingestion, rate limiting, retention,
durable storage, private remote-monitoring endpoints and deployment. It has no
production credentials and must not be deployed as a statistics service.
