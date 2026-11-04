import unittest

from inventory.models import Item


class ItemTests(unittest.TestCase):
    def record(self, **overrides):
        record = {"sku": "A1", "name": " Widget ", "quantity": "3", "unit_price": "2.50"}
        record.update(overrides)
        return record

    def test_from_record_parses_fields(self):
        item = Item.from_record(self.record())
        self.assertEqual(item.name, "Widget")
        self.assertEqual(item.quantity, 3)
        self.assertEqual(item.unit_price, 2.5)
        self.assertEqual(item.status, "active")

    def test_discontinued_item_from_record(self):
        item = Item.from_record(self.record(status=" Discontinued "))
        self.assertTrue(item.discontinued)

    def test_active_item_is_not_discontinued(self):
        self.assertFalse(Item.from_record(self.record()).discontinued)

    def test_value(self):
        self.assertEqual(Item("A1", "Widget", 4, 2.5).value, 10.0)


if __name__ == "__main__":
    unittest.main()
