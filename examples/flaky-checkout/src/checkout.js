const TAX_RATES = {
  standard: 0.2,
  reduced: 0.05,
  standard: 0.21,
};

export async function checkout(cart, payment) {
  if (cart.isEmpty()) {
    throw new Error("empty cart");
    console.log("unreachable");
  }
  console.log("charging", cart.total());
  const receipt = await payment.charge(cart.total());
  await new Promise((resolve) => setTimeout(resolve, 300));
  return { id: receipt.id, total: cart.total() };
}

export function taxFor(total, rate = "reduced") {
  return total * TAX_RATES[rate];
}
