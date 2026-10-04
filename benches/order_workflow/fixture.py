#!/usr/bin/env python3
"""Deterministic shared SQLite inputs and an independent relational oracle."""
import argparse
import random
import sqlite3
from pathlib import Path

SEED = 0x6A09E667F3BCC909
AUDIT_COLUMNS = 'line_id,order_id,customer_id,name,region,day,status,quantity,unit_price,amount_cents,errors'
VARIANT_COLUMNS = 'line_id,name,region,day,status,amount_cents'
PARAMETERS = [
    (-1, 0, 366, -1, 0, 0),
    (0, 0, 366, -1, 500, 125),
    (1, 30, 300, 1, 1000, 500),
    (2, 90, 270, 2, 2000, 1000),
    (3, 0, 180, -1, 0, 250),
    (-1, 180, 366, 0, 1000, 750),
    (-1, 0, 366, -1, 5000, 1500),
    (0, 100, 200, 1, 0, 2000),
]
SCHEMA = '''
CREATE TABLE customers(id INTEGER NOT NULL, name TEXT, region INTEGER NOT NULL, active INTEGER NOT NULL);
CREATE UNIQUE INDEX customer_key ON customers(id);
CREATE TABLE orders(id INTEGER NOT NULL, customer_id INTEGER, day INTEGER, status INTEGER NOT NULL);
CREATE UNIQUE INDEX order_key ON orders(id);
CREATE TABLE lines(id INTEGER NOT NULL, order_id INTEGER, quantity INTEGER, unit_price INTEGER);
CREATE TABLE parameters(id INTEGER, region INTEGER, from_day INTEGER, to_day INTEGER, status INTEGER, minimum INTEGER, discount_bp INTEGER);
CREATE VIEW expected_audit AS
SELECT l.id AS line_id, l.order_id, o.customer_id, c.name, c.region, o.day, o.status,
       l.quantity, l.unit_price,
       CASE WHEN l.quantity > 0 AND l.unit_price >= 0 THEN l.quantity*l.unit_price END AS amount_cents,
       (CASE WHEN o.id IS NULL THEN 1 ELSE 0 END) |
       (CASE WHEN o.id IS NOT NULL AND c.id IS NULL THEN 2 ELSE 0 END) |
       (CASE WHEN c.id IS NOT NULL AND (c.name IS NULL OR c.name = '') THEN 4 ELSE 0 END) |
       (CASE WHEN c.id IS NOT NULL AND c.active = 0 THEN 8 ELSE 0 END) |
       (CASE WHEN l.quantity IS NULL OR l.quantity <= 0 THEN 16 ELSE 0 END) |
       (CASE WHEN l.unit_price IS NULL OR l.unit_price < 0 THEN 32 ELSE 0 END) |
       (CASE WHEN o.id IS NOT NULL AND o.day IS NULL THEN 64 ELSE 0 END) AS errors,
       l.rowid AS source_pos
FROM lines l LEFT JOIN orders o ON l.order_id=o.id LEFT JOIN customers c ON o.customer_id=c.id;
CREATE VIEW expected_clean AS SELECT * FROM expected_audit WHERE errors=0;
CREATE VIEW expected_variants AS
SELECT a.line_id,a.name,a.region,a.day,a.status,
       (a.amount_cents*(10000-p.discount_bp)+5000)/10000 AS amount_cents,
       a.source_pos, p.id AS parameter_id
FROM expected_clean a CROSS JOIN parameters p
WHERE (p.region=-1 OR a.region=p.region) AND a.day>=p.from_day AND a.day<p.to_day
  AND (p.status=-1 OR a.status=p.status) AND a.amount_cents>=p.minimum;
'''


def generate(path, rows=10000, small=False):
    path = Path(path)
    if path.exists():
        raise FileExistsError(f'Refusing to replace {path}; choose a new output path')
    path.parent.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(path)
    db.executescript('PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;' + SCHEMA)
    db.executemany('INSERT INTO parameters VALUES (?,?,?,?,?,?,?)', enumerate_parameters())
    if small:
        db.executemany('INSERT INTO customers VALUES (?,?,?,?)',
                       [(20, 'Åsa', 0, 1), (10, '', 1, 0), (30, None, 2, 1)])
        db.executemany('INSERT INTO orders VALUES (?,?,?,?)',
                       [(200, 20, 100, 1), (100, 10, 180, 0), (300, 21, None, 2),
                        (400, 30, 99, 1), (500, None, 200, 1),
                        (600, 20, 200, 1), (700, 20, 99, 1)])
        db.executemany('INSERT INTO lines VALUES (?,?,?,?)', [
            (8, 200, 2, 250), (2, 201, 1, 100), (4, None, 1, 100),
            (6, 100, 0, -1), (10, 300, None, None), (12, 400, -1, 100),
            (14, 500, 1, 100), (16, 200, 1, 0), (18, 200, 1, 5000),
            (20, 600, 1, 500), (22, 700, 1, 500),
        ])
    else:
        if rows < 20 or rows > 100_000_000:
            raise ValueError('rows must be in 20..100000000')
        rng = random.Random(SEED)
        customers = max(1, rows // 20)
        orders = max(1, rows // 5)
        def shuffled(count):
            ids = list(range(count))
            rng.shuffle(ids)
            return ids
        db.executemany('INSERT INTO customers VALUES (?,?,?,?)', (
            (2*(i+1), None if i % 23 == 0 else '' if i % 29 == 0 else f'Kund Åsa {i}',
             i % 4, int(i % 19 != 0)) for i in shuffled(customers)))
        db.executemany('INSERT INTO orders VALUES (?,?,?,?)', (
            (2*(i+1), None if i % 43 == 0 else 2*((i*17)%customers+1)+(i % 37 == 0),
             None if i % 67 == 0 else i % 366, i % 3) for i in shuffled(orders)))
        db.executemany('INSERT INTO lines VALUES (?,?,?,?)', (
            (2*(i+1), None if i % 43 == 0 else 2*((i*31)%orders+1)+(i % 41 == 0),
             None if i % 47 == 0 else 0 if i % 31 == 0 else -1 if i % 53 == 0 else i % 5+1,
             None if i % 59 == 0 else -1 if i % 61 == 0 else (i*97)%10001)
            for i in shuffled(rows)))
    db.commit()
    if small:
        # Hand-calculated expectations, independent of both application implementations.
        assert db.execute('SELECT line_id,errors FROM expected_audit ORDER BY source_pos').fetchall() == [
            (8, 0), (2, 1), (4, 1), (6, 60), (10, 114), (12, 20), (14, 2), (16, 0), (18, 0), (20, 0), (22, 0)]
        assert db.execute('SELECT line_id,amount_cents FROM expected_variants WHERE parameter_id=1 ORDER BY source_pos').fetchall() == [(8, 494), (18, 4938), (20, 494), (22, 494)]
        assert db.execute('SELECT line_id FROM expected_variants WHERE parameter_id=7 ORDER BY source_pos').fetchall() == [(8,), (16,), (18,)]
    db.close()


def enumerate_parameters():
    return [(i, *p) for i, p in enumerate(PARAMETERS)]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output')
    parser.add_argument('--rows', type=int, default=10000)
    parser.add_argument('--small', action='store_true')
    args = parser.parse_args()
    generate(args.output, args.rows, args.small)
