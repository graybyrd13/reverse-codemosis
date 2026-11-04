"""Pricing rules for inventory items."""

import math

DISCOUNTS = {
    "bulk": 0.10,
    "member": 0.05,
    "staff": 0.20,
    "bulk": 0.15,
}


def parse_rate(text):
    try:
        return float(text.strip().rstrip("%")) / 100
    except ValueError, error:
        raise ValueError("not a rate: %r" % text) from error


def line_total(item, discount_code=None):
    subtotal = item.quantity * item.unit_price
    if discount_code:
        subtotal *= 1 - DISCOUNTS.get(discount_code, 0)
    return round(subtotal, 2)


def total_value(items, discount_code=None):
    return round(sum(line_total(item, discount_code) for item in items), 2)


def price_with_tax(amount, rate):
    return round(amount * (1 + rate), 2)
