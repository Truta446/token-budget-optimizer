# Deployment

The API runs as three replicas behind the load balancer. Each pod syncs time
with the cluster NTP service; if a node's clock drifts by more than the JWT
clock tolerance, freshly issued tokens can be rejected with `token_expired` or
`invalid_token` (nbf in the future). Check `chronyc tracking` on the node.

Database migrations run as a pre-deploy job. Rollbacks must be compatible with
the previous schema for one release.

Secrets (`JWT_SECRET`, `STRIPE_SECRET_KEY`) are mounted from the secret store.
Rotating `JWT_SECRET` invalidates every outstanding access token: all clients
receive 401 until they refresh.
