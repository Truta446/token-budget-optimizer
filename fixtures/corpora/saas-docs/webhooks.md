# Webhooks

Webhooks notify your service when events happen in Acme Cloud, for example
`deployment.succeeded` or `invoice.paid`.

## Signature verification
Every request includes an `Acme-Signature` header: an HMAC-SHA256 of the raw
request body using your endpoint's signing secret, hex encoded, prefixed with
the timestamp (`t=1700000000,v1=...`). Reject requests whose timestamp is more
than 5 minutes old to prevent replay attacks. Always compute the HMAC over the
raw body bytes, before JSON parsing.

## Retries
Endpoints must respond with a 2xx status within 10 seconds. Failed deliveries
are retried with exponential backoff for up to 72 hours (12 attempts). After
that the endpoint is disabled and workspace owners are emailed.

## Ordering and duplicates
Events can arrive out of order and can be delivered more than once. Use the
`event.id` field to deduplicate.
