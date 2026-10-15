# Billing and plans

## Plans
Acme Cloud offers three plans. **Starter** is free for up to 3 projects and
10,000 API requests per month. **Team** costs $49 per seat per month and
includes 1 million API requests. **Enterprise** pricing is negotiated and
includes SSO, audit logs, and a 99.95% uptime SLA.

## Invoices
Invoices are issued on the first day of each billing cycle and sent to the
billing email configured under *Settings → Billing*. Past invoices can be
downloaded as PDF for 24 months.

## Changing plans
Upgrades take effect immediately and are prorated to the day. Downgrades take
effect at the end of the current billing cycle. If you downgrade to Starter
while having more than 3 projects, the oldest projects become read-only.

## Failed payments
When a card payment fails we retry after 3, 5 and 7 days. After the final
failed retry the workspace is suspended: data is kept for 30 days, but API
requests return HTTP 402 until the balance is paid.

## Refunds
Annual plans can be refunded pro rata within 30 days of purchase. Monthly plans
are not refundable. Contact billing@acme.example to request a refund.
