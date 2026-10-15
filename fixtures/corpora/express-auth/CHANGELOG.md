# Changelog

## 2.4.0
- Orders can be cancelled until they ship (`POST /orders/:id/cancel`).
- Added nightly cleanup of revoked and expired sessions.

## 2.3.1
- Fixed a race where two concurrent refresh requests could both succeed.
- Access tokens now include `jti` for revocation lists.

## 2.3.0
- Moved payments to Stripe Checkout.
- Added per-IP rate limiting (120 requests per minute).
