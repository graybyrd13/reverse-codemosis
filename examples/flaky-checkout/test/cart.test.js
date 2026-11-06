import { test } from "node:test";
import assert from "node:assert/strict";
import { Cart } from "../src/cart.js";

test("the total adds up every item", async () => {
  const cart = new Cart();
  cart.add("A1", 5);
  cart.add("B2", 2, 3);
  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(cart.total(), 11);
});

test("a new cart is empty", () => {
  assert.equal(new Cart().isEmpty(), true);
});

test("a price that is not a number is detected", () => {
  const cart = new Cart();
  cart.add("A1", Number("twelve"));
  assert.equal(cart.hasInvalidPrices(), true);
});
