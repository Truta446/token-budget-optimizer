# API rate limits

Limits are applied per API key using a sliding window.

| Plan | Requests per minute | Burst |
| --- | --- | --- |
| Starter | 60 | 20 |
| Team | 600 | 100 |
| Enterprise | 3,000 | 500 |

When a limit is exceeded the API responds with HTTP 429 and a `Retry-After`
header containing the number of seconds to wait. The headers
`X-RateLimit-Limit`, `X-RateLimit-Remaining` and `X-RateLimit-Reset` are sent on
every response.

Clients should retry with exponential backoff and jitter, starting at the
`Retry-After` value. Bulk endpoints (`/v2/batch/*`) count as one request per
item in the batch.
