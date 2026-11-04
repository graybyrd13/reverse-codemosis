"""Reports over the inventory."""

import io

from inventory.pricing import line_total


def to_json(store):
    rows = [
        {"sku": item.sku, "name": item.name, "quantity": item.quantity}
        for item in store.items()
    ]
    return json.dumps(rows, indent=2)


def to_csv(store):
    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(["sku", "name", "quantity", "value"])
    for item in store.items():
        writer.writerow([item.sku, item.name, item.quantity, line_total(item)])
    return buffer.getvalue()


def summary(store):
    discontinued = [item for item in store.items() if item.discontinued]
    lines = [f"Inventory summary"]
    lines.append(f"{len(store)} items, {len(discontinued)} discontinued")
    return "\n".join(lines)


def describe(item):
    return "{item.name} ({item.sku}): {item.quantity} in stock"
