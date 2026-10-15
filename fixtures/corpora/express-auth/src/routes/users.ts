import { Router } from "express";
import { z } from "zod";
import bcrypt from "bcryptjs";
import { db } from "../db/pool.js";
import { authenticate } from "../middleware/authenticate.js";

export const users = Router();

const SignupBody = z.object({
  email: z.string().email().max(254),
  password: z.string().min(12).max(128),
  name: z.string().min(1).max(100),
});

users.post("/signup", async (req, res) => {
  const parsed = SignupBody.safeParse(req.body);
  if (!parsed.success) return res.status(400).json({ error: "invalid_body", issues: parsed.error.issues });
  const hash = await bcrypt.hash(parsed.data.password, 12);
  const user = await db.one(
    "INSERT INTO users (email, password_hash, name) VALUES ($1, $2, $3) RETURNING id, email, name",
    [parsed.data.email.toLowerCase(), hash, parsed.data.name],
  );
  res.status(201).json(user);
});

users.get("/me", authenticate, async (req, res) => {
  const user = await db.one("SELECT id, email, name, created_at FROM users WHERE id = $1", [req.user!.sub]);
  res.json(user);
});
