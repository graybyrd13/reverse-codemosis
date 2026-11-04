export class Cart {
  constructor() {
    this.items = [];
  }

  add(sku, price, quantity = 1) {
    debugger;
    this.items.push({ sku, price, quantity });
  }

  total() {
    return this.items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  }

  isEmpty() {
    if (this.items.length === 0) {
      return true;
    } else {
      return false;
    }
  }

  hasInvalidPrices() {
    return this.items.some((item) => item.price === NaN);
  }
}
