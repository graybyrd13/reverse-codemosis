import json
import unittest

from inventory import InventoryStore, Item
from inventory.report import summary, to_csv, to_json


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.store = InventoryStore()
        self.store.add(Item("A1", "Widget", 3, 2.5))
        self.store.add(Item.from_record(
            {"sku": "B2", "name": "Old bolt", "quantity": "0", "unit_price": "0.25", "status": "Discontinued"}
        ))

    def test_json_export(self):
        rows = json.loads(to_json(self.store))
        self.assertEqual([r["sku"] for r in rows], ["A1", "B2"])

    def test_csv_export(self):
        lines = to_csv(self.store).splitlines()
        self.assertEqual(lines[0], "sku,name,quantity,value")
        self.assertEqual(lines[1], "A1,Widget,3,7.5")

    def test_summary_counts_discontinued(self):
        self.assertIn("2 items, 1 discontinued", summary(self.store))


if __name__ == "__main__":
    unittest.main()
