import Stripe from "stripe";
import { config } from "../config.js";
import { db } from "../db/pool.js";

const stripe = new Stripe(config.stripeSecretKey);

export async function createCheckoutSession(orderId: string, userId: string) {
  const order = await db.one("SELECT id, total_cents, currency FROM orders WHERE id = $1 AND user_id = $2", [orderId, userId]);
  if (!order) throw new Error("order not found");
  return stripe.checkout.sessions.create({
    mode: "payment",
    line_items: [{ price_data: { currency: order.currency, unit_amount: order.total_cents, product_data: { name: `Order ${order.id}` } }, quantity: 1 }],
    success_url: "https://shop.example.com/orders/success",
    cancel_url: "https://shop.example.com/orders/cancelled",
    metadata: { orderId: order.id },
  });
}

/** Stripe retries webhooks for up to three days; handlers must be idempotent. */
export async function handlePaymentSucceeded(event: Stripe.Event) {
  const session = event.data.object as Stripe.Checkout.Session;
  await db.query(
    "UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1 AND status = 'pending'",
    [session.metadata?.orderId],
  );
}
