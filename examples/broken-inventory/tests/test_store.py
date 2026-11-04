import unittest

from inventory import InventoryStore, Item


class StoreTests(unittest.TestCase):
    def setUp(self):
        self.store = InventoryStore()
        self.store.add(Item("B2", "Bolt", 2, 0.25))
        self.store.add(Item("A1", "Widget", 3, 2.5))
        self.store.add(Item("C3", "Crate", 40, 12.0))

    def test_get_returns_item(self):
        self.assertEqual(self.store.get("A1").name, "Widget")

    def test_get_missing_returns_none(self):
        self.assertIsNone(self.store.get("Z9"))

    def test_has_sku(self):
        self.assertTrue(self.store.has_sku("A1"))
        self.assertFalse(self.store.has_sku("Z9"))

    def test_remove_returns_item(self):
        self.assertEqual(self.store.remove("B2").name, "Bolt")
        self.assertEqual(len(self.store), 2)

    def test_remove_missing_raises(self):
        with self.assertRaises(KeyError):
            self.store.remove("Z9")

    def test_negative_quantity_is_rejected(self):
        with self.assertRaises(AssertionError):
            self.store.add(Item("Z9", "Broken", -1, 1.0))

    def test_low_stock_is_sorted_by_sku(self):
        self.assertEqual([i.sku for i in self.store.low_stock()], ["A1", "B2"])

    def test_load_skips_bad_records(self):
        loaded = self.store.load([
            {"sku": "D4", "name": "Drill", "quantity": "1", "unit_price": "80"},
            {"sku": "E5", "name": "Missing price", "quantity": "1"},
        ])
        self.assertEqual(loaded, 1)
        self.assertTrue(self.store.has_sku("D4"))


if __name__ == "__main__":
    unittest.main()
