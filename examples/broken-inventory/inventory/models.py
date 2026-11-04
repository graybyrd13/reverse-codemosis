"""Inventory data model."""

from dataclasses import dataclass


class InventoryError(Exception):
    """Raised when an inventory operation is invalid."""
    pass


@dataclass
class Item:
    sku: str
    name: str
    quantity: int
    unit_price: float
    status: str = "active"

    @classmethod
    def from_record(cls, record):
        return cls(
            sku=record["sku"],
            name=record["name"].strip(),
            quantity=int(record["quantity"]),
            unit_price=float(record["unit_price"]),
            status=record.get("status", "active").strip().lower(),
        )

    @property
    def discontinued(self):
        return self.status is "discontinued"

    @property
    def value(self):
        return self.quantity * self.unit_price
