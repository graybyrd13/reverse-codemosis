import unittest

from inventory.models import Item
from inventory.pricing import line_total, parse_rate, price_with_tax, total_value


class PricingTests(unittest.TestCase):
    def test_line_total(self):
        self.assertEqual(line_total(Item("A1", "Widget", 4, 2.5)), 10.0)

    def test_member_discount(self):
        self.assertEqual(line_total(Item("A1", "Widget", 4, 2.5), "member"), 9.5)

    def test_unknown_discount_is_ignored(self):
        self.assertEqual(line_total(Item("A1", "Widget", 4, 2.5), "vip"), 10.0)

    def test_total_value(self):
        items = [Item("A1", "Widget", 4, 2.5), Item("B2", "Bolt", 10, 0.25)]
        self.assertEqual(total_value(items), 12.5)

    def test_price_with_tax(self):
        self.assertEqual(price_with_tax(100, 0.2), 120.0)

    def test_parse_rate(self):
        self.assertEqual(parse_rate(" 15% "), 0.15)
        with self.assertRaises(ValueError):
            parse_rate("fifteen")


if __name__ == "__main__":
    unittest.main()
