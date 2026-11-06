import { test } from "node:test";
import assert from "node:assert/strict";
import { Cart } from "../src/cart.js";
import { checkout, taxFor } from "../src/checkout.js";

const payment = { charge: async (amount) => ({ id: "r-1", amount }) };

test("checkout returns a receipt", async () => {
  const cart = new Cart();
  cart.add("A1", 10, 2);
  assert.deepEqual(await checkout(cart, payment), { id: "r-1", total: 20 });
});

test("checkout refuses an empty cart", async () => {
  await assert.rejects(checkout(new Cart(), payment), /empty cart/);
});

test("reduced tax", () => {
  assert.equal(taxFor(100), 5);
});
