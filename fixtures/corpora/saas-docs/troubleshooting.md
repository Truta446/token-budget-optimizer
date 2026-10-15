# Troubleshooting

## 401 Unauthorized
The API key is missing, revoked, or sent in the wrong header. Send it as
`Authorization: Bearer <key>`. Keys created before 2023 use the `ak_` prefix
and still work; keys are shown only once at creation.

## 403 Forbidden
The key is valid but lacks the required scope. Edit the key under
*Settings → API keys* and add the scope, e.g. `deployments:write`.

## 402 Payment Required
The workspace is suspended after failed payments. See *Billing → Failed payments*.

## Slow deployments
Builds are queued per workspace. Starter workspaces run one build at a time;
Team runs three concurrent builds; Enterprise runs ten.
