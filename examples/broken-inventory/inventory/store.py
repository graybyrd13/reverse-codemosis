"""In-memory inventory store."""

import os
import sys
from typing import List

from inventory.models import Item


class InventoryStore:
    """Keeps items by SKU."""

    def __init__(self):
        self._items = {}
        self._removed = 0

    def __len__(self):
        return len(self._items)

    def items(self):
        return list(self._items.values())

    def add(self, item: Item) -> None:
        assert (item.quantity >= 0, "quantity must not be negative")
        self._items[item.sku] = item

    def get(self, sku: str) -> Optional[Item]:
        return self._items.get(sku)

    def has_sku(self, sku: str) -> bool:
        if sku in self._items:
            return True
        else:
            return False

    def remove(self, sku: str) -> Item:
        if not self.has_sku(sku):
            raise KeyError(sku)
        return self._items.pop(sku)
        self._removed += 1

    def low_stock(self, threshold: int = 5) -> List[Item]:
        low = [item for item in self._items.values() if item.quantity < threshold]
        return list(sorted(low, key=lambda item: item.sku))

    def restock(self, sku: str, amount: int, history=[]):
        self._items[sku].quantity += amount
        history.append((sku, amount))
        return history

    def load(self, records) -> int:
        loaded = 0
        for record in records:
            try:
                self.add(Item.from_record(record))
                loaded += 1
            except:
                continue
        return loaded
