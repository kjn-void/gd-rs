"""Render the five M6 filter-copy representations and their string ownership."""
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch, Rectangle

OUT = Path(__file__).resolve().parents[2] / 'docs/high-level/examples/images'
plt.rcParams.update({'font.family': 'DejaVu Sans', 'svg.fonttype': 'none',
                     'svg.hashsalt': 'gd-m6-comparison-layout'})
fig, ax = plt.subplots(figsize=(22, 20))
fig.subplots_adjust(left=0, right=1, top=1, bottom=0)
ax.set_xlim(0, 2100)
ax.set_ylim(1900, 0)
ax.axis('off')
fig.patch.set_facecolor('white')
C = {'ink': '#18334c', 'muted': '#526778', 'line': '#c8d4dd',
     'blue': '#146caa', 'copy': '#dcedf8', 'orange': '#a65508',
     'filter': '#fff0d6', 'heap': '#e4f2eb', 'green': '#337455',
     'shared': '#eee6f7', 'purple': '#79529a', 'empty': '#f5f7f9'}
ROWS = [4, 5, 6]
MATCHES = [4, 6]
VALUES = {4: [4, 48, 59], 5: [5, 85, 72], 6: [6, 22, 85]}


def text(x, y, value, size=12, color=None, weight='normal', ha='left', **kwargs):
    return ax.text(x, y, value, fontsize=size, color=color or C['ink'],
                   fontweight=weight, ha=ha, va='center', **kwargs)


def box(x, y, width, height, face='white', edge=None, rounded=False):
    args = dict(facecolor=face, edgecolor=edge or C['line'], linewidth=.9)
    if rounded:
        patch = FancyBboxPatch((x, y), width, height,
                              boxstyle='round,pad=0,rounding_size=12', **args)
    else:
        patch = Rectangle((x, y), width, height, **args)
    ax.add_patch(patch)


def arrow(start, end, color=None, route='arc3,rad=0', width=1.5):
    ax.add_patch(FancyArrowPatch(start, end, arrowstyle='-|>', mutation_scale=12,
                                color=color or C['blue'], linewidth=width,
                                connectionstyle=route))


def card(x, y, title, subtitle):
    box(x, y, 990, 510, rounded=True)
    text(x + 25, y + 35, title, 19, weight='bold')
    text(x + 25, y + 69, subtitle, 11.5, color=C['muted'])


def headers(x, y):
    text(x + 30, y + 109, 'SOURCE', 11, weight='bold', color=C['muted'])
    text(x + 660, y + 109, 'DESTINATION', 11, weight='bold', color=C['muted'])


def operation(x, y, title, note):
    text(x + 493, y + 154, title, 13, ha='center', weight='bold')
    arrow((x + 356, y + 190), (x + 637, y + 190), width=2)
    text(x + 493, y + 232, note, 11.5, ha='center', linespacing=1.5,
         color=C['muted'])


def aos(x, y, rows, pointers=False, destination=False):
    widths = [42, 59, 56, 75, 75]
    labels = ['id', 'selector', 'amount', 'name', 'message']
    cursor = x
    for label, width in zip(labels, widths):
        text(cursor + width / 2, y - 14, label, 8.7, ha='center', weight='bold')
        cursor += width
    for index, row in enumerate(rows):
        cursor = x
        for column, width in enumerate(widths):
            fill = C['copy'] if row in MATCHES else C['empty']
            if column == 1 and not destination:
                fill = C['filter']
            box(cursor, y + index * 36, width, 34, fill)
            value = VALUES[row][column] if column < 3 else ('ptr' if pointers else f'{"n" if column == 3 else "m"}{row}')
            text(cursor + width / 2, y + index * 36 + 17, str(value), 11, ha='center',
                 color=C['orange'] if column == 1 and not destination else C['ink'])
            cursor += width
    return widths


def heaps(x, y, object_centers, row_bottom):
    for index, (label, start) in enumerate(zip(['name text…', 'message text…'], object_centers)):
        xpos = x + index * 159
        arrow((start, row_bottom), (xpos + 74, y - 3), C['green'])
        box(xpos, y, 147, 38, C['heap'])
        text(xpos + 73.5, y + 19, label, 10.5, ha='center')


def soa(x, y, rows, descriptors=False, destination=False):
    labels = ['id', 'selector', 'amount', 'name', 'message']
    start = x + 85
    for index, row in enumerate(rows):
        text(start + index * 71 + 34.5, y - 14, f'row {row}', 9, ha='center', color=C['muted'])
    for column, label in enumerate(labels):
        ypos = y + column * 33
        text(x, ypos + 15, label, 10.5, weight='bold')
        for index, row in enumerate(rows):
            fill = C['copy'] if row in MATCHES else C['empty']
            if column == 1 and not destination:
                fill = C['filter']
            box(start + index * 71, ypos, 69, 30, fill)
            value = VALUES[row][column] if column < 3 else ('off / len' if descriptors else 'ptr')
            text(start + index * 71 + 34.5, ypos + 15, str(value),
                 9 if descriptors and column >= 3 else 11, ha='center',
                 color=C['orange'] if column == 1 and not destination else C['ink'])
    return start, start + (len(rows) - 1) * 71 + 34.5


def slots(x, y, rows, label):
    text(x, y + 16, label, 9.5, weight='bold')
    for index, row in enumerate(rows):
        box(x + 85 + index * 71, y, 71, 33,
            C['copy'] if row in MATCHES else C['empty'])
        text(x + 120.5 + index * 71, y + 16, f'{"n" if label.startswith("name") else "m"}{row}',
             11, ha='center')


text(45, 46, 'One filter, five ways to store and copy records', 29, weight='bold')
text(45, 91, 'M6 comparison  ·  three numeric fields + two text fields  ·  one source and one ordered destination',
     14, color=C['muted'])
box(45, 126, 2010, 58, C['filter'], rounded=True)
text(72, 155, 'FILTER  ·  selector < 50', 16, weight='bold', color=C['orange'])
text(546, 155, 'row 4: 48 ✓     row 5: 85 ×     row 6: 22 ✓', 14)
arrow((1122, 155), (1248, 155), C['orange'])
text(1290, 155, 'Copy or share rows 4 and 6, in that order', 14, weight='bold')

# 1. GD owns all text inside its row buffer, so the row is a byte-copy unit.
x, y = 45, 213
card(x, y, '1 · GD memcpy', 'AoS · complete records in one contiguous row buffer')
headers(x, y)
aos(x + 30, y + 149, ROWS)
aos(x + 660, y + 149, MATCHES, destination=True)
operation(x, y, 'memcpy each row', 'Numbers + both strings\ntravel together')
text(x + 30, y + 282, 'Text lives inside each row.', 12, weight='bold')
text(x + 660, y + 282, 'Independent copied rows.', 12, weight='bold')
box(x + 30, y + 332, 930, 93, C['copy'], rounded=True)
text(x + 51, y + 360, 'Short and long strings use bounded inline row slots.', 14, weight='bold')
text(x + 51, y + 395, 'Each matched row carries its text and metadata into the new row buffer.', 12)
text(x + 30, y + 475, 'The filter reads selectors separated by the other fields of each record.',
     11.5, color=C['muted'])

# 2. STL owns each string through its standard copy operation.
x, y = 1065, 213
card(x, y, '2 · C++ STL std::string', 'AoS · std::vector<StringRow> with ordinary owning string objects')
headers(x, y)
aos(x + 30, y + 149, ROWS, pointers=True)
aos(x + 660, y + 149, MATCHES, pointers=True, destination=True)
operation(x, y, 'Copy each record', 'Copy numeric fields\nand std::string values')
heaps(x + 30, y + 310, [x + 224.5, x + 299.5], y + 255)
heaps(x + 660, y + 310, [x + 854.5, x + 929.5], y + 219)
text(x + 30, y + 374, 'Source-owned heap text', 11, color=C['green'])
text(x + 660, y + 374, 'New destination heap text', 11, color=C['green'])
text(x + 30, y + 421, 'Long strings: allocate and copy their characters.', 13, weight='bold')
text(x + 30, y + 451, 'Short strings: characters stay inside the string object (SSO).', 12)
text(x + 30, y + 480, 'Heap blocks shown for one example row; each long string has its own allocation.',
     10.5, color=C['muted'])

# 3. The dynamic Table keeps string objects in their own contiguous columns.
x, y = 45, 743
card(x, y, '3 · gd-rs CompactString', 'SoA · five separate columns; string objects live in the text columns')
headers(x, y)
soa(x + 30, y + 145, ROWS)
soa(x + 660, y + 145, MATCHES, destination=True)
operation(x, y, 'Gather columns', 'Copy numeric values\nand clone strings')
for xpos in [x + 30, x + 660]:
    # Show the two strings of the first selected record. Route the name pointer
    # beside the column cells so it cannot look like a message-column pointer.
    ax.plot([xpos + 84, xpos + 78, xpos + 78], [y + 259, y + 259, y + 332],
            color=C['green'], linewidth=1.2)
    arrow((xpos + 78, y + 332), (xpos + 73.5, y + 346), C['green'])
    arrow((xpos + 119.5, y + 309), (xpos + 232.5, y + 346), C['green'])
    for index, label in enumerate(['name text…', 'message text…']):
        box(xpos + index * 159, y + 349, 147, 38, C['heap'])
        text(xpos + index * 159 + 73.5, y + 368, label, 10.5, ha='center')
text(x + 30, y + 415, 'Long strings: each copied string gets its own heap text.', 13, weight='bold')
text(x + 30, y + 445, 'Short strings: characters are inline in each CompactString object.', 12)
text(x + 30, y + 479, 'Selectors are adjacent. Matching cells are gathered into each destination column.',
     11.5, color=C['muted'])

# 4. Fixed strings are offsets and lengths into one byte buffer per column.
x, y = 1065, 743
card(x, y, '4 · gd-rs fixed buffer', 'SoA · numeric columns + string descriptors pointing into fixed-slot buffers')
headers(x, y)
soa(x + 30, y + 145, ROWS, descriptors=True)
soa(x + 660, y + 145, MATCHES, descriptors=True, destination=True)
operation(x, y, 'Gather columns', 'Copy text into new slots\nand write new offsets')
for xpos, rows in [(x + 30, ROWS), (x + 660, MATCHES)]:
    right = xpos + 85 + len(rows) * 71
    for index, label in enumerate(['name buffer', 'msg buffer']):
        ysource = y + 145 + (index + 3) * 33 + 15
        yslot = y + 350 + index * 47
        ax.plot([right + 2, right + 12 + index * 8, right + 12 + index * 8, right + 2],
                [ysource, ysource, yslot + 16, yslot + 16], color=C['green'], linewidth=1.1)
        arrow((right + 14, yslot + 16), (right, yslot + 16), C['green'])
        slots(xpos, yslot, rows, label)
text(x + 30, y + 462, 'One contiguous text buffer per string column; independent destination buffers.',
     12, weight='bold')
text(x + 30, y + 488, 'Short and long text use the same slot-based layout. No allocation per string.',
     11.5, color=C['muted'])

# 5. Separate vectors own handles pointing at exactly the same record allocations.
x, y = 45, 1273
card(x, y, '5 · gd-rs Arc', 'SharedRecordTable<Record> · one vector of handles to complete records')
text(x + 30, y + 110, 'SOURCE HANDLES', 10.5, color=C['muted'], weight='bold')
text(x + 375, y + 110, 'SHARED RECORDS', 10.5, color=C['muted'], weight='bold')
text(x + 812, y + 110, 'TARGET HANDLES', 10.5, color=C['muted'], weight='bold')
for index, row in enumerate(ROWS):
    ypos = y + 148 + index * 70
    fill = C['shared'] if row in MATCHES else C['empty']
    box(x + 40, ypos, 105, 40, fill)
    text(x + 92.5, ypos + 20, f'Arc → {row}', 11.5, ha='center')
    arrow((x + 148, ypos + 20), (x + 260, ypos + 20), C['purple'])
    box(x + 265, ypos - 5, 450, 51, fill)
    text(x + 282, ypos + 9, f'Record {row}', 10.5, weight='bold')
    text(x + 282, ypos + 31, f'id {row}  ·  selector {VALUES[row][1]}  ·  amount {VALUES[row][2]}  ·  name  ·  message', 10.5)
for index, row in enumerate(MATCHES):
    ypos = y + 148 + index * 45
    box(x + 837, ypos, 105, 40, C['shared'])
    text(x + 889.5, ypos + 20, f'Arc → {row}', 11.5, ha='center')
    arrow((x + 834, ypos + 20), (x + 719, y + 168 + ROWS.index(row) * 70), C['purple'])
text(x + 265, y + 359, 'Both tables point to the same records and string payloads.', 12,
     weight='bold', color=C['purple'])
box(x + 30, y + 391, 930, 54, C['shared'], rounded=True)
text(x + 495, y + 418, 'Filter records → clone Arc handles → concatenate local buffers',
     13, weight='bold', ha='center')
text(x + 30, y + 477, 'Atomic reference counts; no payload copy. With 8 workers, Rayon builds and releases handles.',
     11.5, color=C['muted'])

# Concept key: SSO describes an object layout, not a different string API.
x, y = 1065, 1273
card(x, y, 'How small-string optimization changes the copy', 'The same string type stores short and long text differently')
text(x + 30, y + 112, 'SHORT STRING · text inside the string object', 12.5, weight='bold')
for xpos in [x + 45, x + 662]:
    box(xpos, y + 143, 280, 71, C['copy'], rounded=True)
    text(xpos + 140, y + 162, 'string object', 10, ha='center', color=C['muted'])
    text(xpos + 140, y + 191, '“short text”', 13, ha='center', weight='bold')
arrow((x + 345, y + 179), (x + 642, y + 179), width=2)
text(x + 495, y + 155, 'copy inline text', 12, ha='center')
text(x + 495, y + 224, 'No separate text allocation', 11, ha='center', color=C['muted'])
text(x + 30, y + 264, 'LONG STRING · object points to heap text', 12.5, weight='bold')
for xpos in [x + 45, x + 662]:
    box(xpos, y + 293, 280, 39, C['copy'])
    text(xpos + 140, y + 312, 'pointer + string metadata', 11, ha='center')
    arrow((xpos + 140, y + 334), (xpos + 140, y + 364), C['green'])
    box(xpos, y + 367, 280, 42, C['heap'])
    text(xpos + 140, y + 388, '“long text … … …”', 12, ha='center')
arrow((x + 345, y + 388), (x + 642, y + 388), C['green'], width=2)
text(x + 495, y + 358, 'allocate + copy text', 12, ha='center')
text(x + 30, y + 448, 'Applies to std::string (libc++ on M6) and CompactString.', 12, weight='bold')
text(x + 30, y + 479, 'Arc shares the whole record, including either string representation.', 11.5,
     color=C['muted'])

for xpos, color, label in [(45, C['copy'], 'Copied values / selected rows'),
                           (610, C['filter'], 'Selector read by the filter'),
                           (1125, C['heap'], 'Separate heap text'),
                           (1560, C['shared'], 'Shared record / Arc handle')]:
    box(xpos, 1821, 23, 23, color)
    text(xpos + 36, 1833, label, 12)
text(45, 1870, 'Conceptual layout: capacities, padding and exact byte sizes omitted. ptr = pointer; off / len = offset and valid length. n4 / m4 abbreviate text.',
     11.5, color=C['muted'])

OUT.mkdir(parents=True, exist_ok=True)
fig.savefig(OUT / 'm6-comparison-memory-layout.png', dpi=150)
svg = OUT / 'm6-comparison-memory-layout.svg'
fig.savefig(svg, metadata={'Date': None})
svg.write_text('\n'.join(line.rstrip() for line in svg.read_text().splitlines()) + '\n')
plt.close(fig)
print(OUT / 'm6-comparison-memory-layout.png')
