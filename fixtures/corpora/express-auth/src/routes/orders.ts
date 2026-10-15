import { Router } from "express";
import { authenticate, requireRole } from "../middleware/authenticate.js";
import { db } from "../db/pool.js";

export const orders = Router();

orders.use(authenticate);

orders.get("/", async (req, res) => {
  const rows = await db.query(
    "SELECT id, status, total_cents, created_at FROM orders WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50",
    [req.user!.sub],
  );
  res.json(rows);
});

orders.post("/:id/cancel", async (req, res) => {
  const order = await db.one("SELECT id, status, user_id FROM orders WHERE id = $1", [req.params.id]);
  if (!order || order.user_id !== req.user!.sub) return res.status(404).json({ error: "not_found" });
  if (order.status === "shipped") return res.status(409).json({ error: "already_shipped" });
  await db.query("UPDATE orders SET status = 'cancelled' WHERE id = $1", [order.id]);
  res.status(204).end();
});

orders.delete("/:id", requireRole("admin"), async (req, res) => {
  await db.query("DELETE FROM orders WHERE id = $1", [req.params.id]);
  res.status(204).end();
});
