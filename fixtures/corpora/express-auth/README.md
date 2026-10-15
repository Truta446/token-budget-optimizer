# orders-api

Express + PostgreSQL API for the web shop.

## Running locally

```bash
cp .env.example .env
docker compose up -d postgres
npm install
npm run migrate
npm run dev
```

The server listens on `PORT` (default 3000).

## Authentication

Clients authenticate with short-lived JWT access tokens (15 minutes by default)
sent as `Authorization: Bearer <token>`. When an access token expires, call
`POST /auth/refresh` with the refresh token to obtain a new pair. Refresh tokens
are single use and expire after 30 days.

## Environment variables

| Name | Description |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection string |
| `JWT_SECRET` | HMAC secret for access tokens |
| `ACCESS_TOKEN_TTL_SECONDS` | Access token lifetime, default 900 |
| `JWT_CLOCK_TOLERANCE_SECONDS` | Allowed clock skew, default 5 |
| `STRIPE_SECRET_KEY` | Stripe API key |
