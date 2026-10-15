# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub Security Advisories
("Report a vulnerability" on the repository's Security tab). Do not open a
public issue. You can expect an acknowledgement within a few days.

## Threat model and guarantees

Token Budget Optimizer processes **untrusted text**. Its guarantees:

- **No code execution.** Document content, ids and metadata are treated as inert
  data. Nothing is evaluated, rendered or interpreted.
- **No I/O.** The library performs no filesystem or network access. Metadata is
  passed through untouched and never used to open paths or URLs.
- **No special tokens.** Text such as `<|endoftext|>` is counted as ordinary text.
- **Header injection is limited.** Document ids are sanitised before they are
  embedded in section headers (control characters such as newlines are replaced
  and length is capped), so an id cannot forge additional headers.
- **Panics do not crash the process.** Native panics are caught at the N-API
  boundary and surface as `TokenBudgetError` with code `TBO_INTERNAL`.

What it does **not** do:

- It does not detect or neutralise prompt injection *inside* document content.
  Selected text is copied verbatim; treat the resulting context with the same
  care as any retrieved text.
- It does not impose input size limits in the library itself. Applications
  exposed to untrusted users should bound input size (the playground shows an
  example: 50 documents, 1.5 M characters total, 3 MB request body).

## Playground

The playground is a local demo, bound to `127.0.0.1` by default, without
authentication. It is stateless, does not log request content, applies a strict
Content-Security-Policy, and rate-limits requests per client.
