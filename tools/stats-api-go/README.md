# Local Go statistics API prototype

This is a local, read-only compatibility prototype for the public website's
`GET /api/summary` endpoint. It returns clearly synthetic sample data; it does
not collect miner reports, persist data, or replace the Cloudflare Worker.

Run it from the prototype directory:

```sh
cd tools/stats-api-go
go run .
```

The server binds to `127.0.0.1:8787` by default. Its `/api/summary` response
uses the fields consumed by the public statistics page, and `/healthz` reports
whether the prototype is running. A different listen address may be provided
with `-addr`, but non-loopback addresses are refused.

To see the synthetic data in the packaged site, open the local `stats.html`
preview with `?statsApi=go` appended to its URL, for example
`file:///path/to/stats.html?statsApi=go`. The override is honored only for
`file:` pages and pages served from localhost; published site URLs always use
the production endpoint.

Run the API contract tests with:

```sh
cd tools/stats-api-go
go test ./...
```

This prototype intentionally omits report ingestion, IP-based rate limiting,
retention, durable storage, private remote-monitoring endpoints, and deployment.
The production collector's source and deployment configuration are not in this
repository, so no production data is read, written, or migrated.
