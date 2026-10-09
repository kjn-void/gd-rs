"""Render the GD versus fixed-array Rust SoA memory-layout illustration."""
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle, FancyBboxPatch, FancyArrowPatch

OUT = Path(__file__).resolve().parents[2] / 'docs/high-level/measurements'
plt.rcParams.update({'font.family': 'DejaVu Sans', 'svg.fonttype': 'none'})
fig, ax = plt.subplots(figsize=(20, 14))
fig.subplots_adjust(left=0, right=1, top=1, bottom=0)
ax.set_xlim(0, 2000)
ax.set_ylim(1400, 0)
ax.axis('off')
fig.patch.set_facecolor('#ffffff')
C = {'ink': '#18334c', 'muted': '#526778', 'line': '#c8d4dd',
     'blue': '#146caa', 'blue_fill': '#dcedf8', 'orange': '#a65508',
     'orange_fill': '#fff0d6', 'meta': '#e9edf0', 'meta_line': '#9aa9b4',
     'empty': '#f7f9fb'}
UNIT = 8.5
FIELDS = [('id', 8), ('selector', 8), ('amount', 8), ('name slot', 24), ('message slot', 24)]
VALUES = {4: [4, 48, 59, 'n4', 'm4'], 5: [5, 85, 72, 'n5', 'm5'],
          6: [6, 22, 85, 'n6', 'm6'], 7: [7, 59, 98, 'n7', 'm7']}
MATCHES = [4, 6]
TARGET_POSITION = {4: 3, 6: 4}  # Earlier matched rows 0, 1 and 3 precede this excerpt.


def text(x, y, value, size=13, color=None, weight='normal', ha='left', va='center', **kwargs):
    return ax.text(x, y, value, fontsize=size, color=color or C['ink'],
                   fontweight=weight, ha=ha, va=va, **kwargs)


def box(x, y, width, height, face, edge=None, hatch=None, rounded=False):
    if rounded:
        patch = FancyBboxPatch((x, y), width, height,
                              boxstyle='round,pad=0,rounding_size=10',
                              facecolor=face, edgecolor=edge or C['line'], linewidth=1)
    else:
        patch = Rectangle((x, y), width, height, facecolor=face,
                          edgecolor=edge or C['line'], linewidth=0.8, hatch=hatch)
    ax.add_patch(patch)


def arrow(a, b, color=None, width=1.7, route='arc3,rad=0'):
    ax.add_patch(FancyArrowPatch(a, b, arrowstyle='-|>', mutation_scale=15,
                                color=color or C['blue'], linewidth=width,
                                connectionstyle=route))


def gd_table(x, y, rows, destination=False):
    start = x + 70
    cursor = start
    for name, width in FIELDS:
        text(cursor + width * UNIT / 2, y - 31, name, 9 if width == 8 else 11, ha='center', weight='bold')
        text(cursor + width * UNIT / 2, y - 10, f'{width} B', 10, ha='center', color=C['muted'])
        cursor += width * UNIT
    for index, row in enumerate(rows):
        ypos = y + index * 43
        selected = row in MATCHES
        text(x + 59, ypos + 20, f'row {TARGET_POSITION[row] if destination else row}', 11, ha='right')
        cursor = start
        for column, (_, width) in enumerate(FIELDS):
            value = VALUES[row][column]
            fill = C['blue_fill'] if selected else C['empty']
            if column == 1 and not destination:
                fill = C['orange_fill']
            if column < 3:
                box(cursor, ypos, width * UNIT, 39, fill)
                text(cursor + width * UNIT / 2, ypos + 20, str(value), 13,
                     color=C['orange'] if column == 1 and not destination else C['ink'], ha='center')
            else:
                # Actual 24-byte GD slot: 4-byte length, 16 payload bytes,
                # then NUL + unused capacity/alignment (4 bytes together).
                box(cursor, ypos, 4 * UNIT, 39, C['meta'], C['meta_line'], '///')
                box(cursor + 4 * UNIT, ypos, 16 * UNIT, 39, fill)
                box(cursor + 20 * UNIT, ypos, 4 * UNIT, 39, C['meta'], C['meta_line'], '///')
                text(cursor + 12 * UNIT, ypos + 20, value + ' · 16 B', 12, ha='center')
            cursor += width * UNIT
    return start


def soa_table(x, y, rows, destination=False):
    start = x + 165
    labels = [('id', 'Vec<u64>', 8), ('selector', 'Vec<u64>', 8),
              ('amount', 'Vec<u64>', 8), ('name', 'Vec<TextCell<16>>', 20),
              ('message', 'Vec<TextCell<16>>', 20)]
    for position, row in enumerate(rows):
        text(start + (position + 0.5) * 8 * UNIT, y - 26,
             str(TARGET_POSITION[row] if destination else row), 10, ha='center', color=C['muted'])
    text(x + 154, y - 26, 'row index', 10, ha='right', color=C['muted'])
    for column, (name, dtype, width) in enumerate(labels):
        ypos = y + column * 43
        text(x, ypos + 12, name, 12, weight='bold')
        text(x, ypos + 31, dtype, 9, color=C['muted'])
        for position, row in enumerate(rows):
            xpos = start + position * width * UNIT
            fill = C['blue_fill'] if row in MATCHES else C['empty']
            if column == 1 and not destination:
                fill = C['orange_fill']
            if column >= 3:
                box(xpos, ypos, 4 * UNIT, 39, C['meta'], C['meta_line'], '///')
                box(xpos + 4 * UNIT, ypos, 16 * UNIT, 39, fill)
                center = xpos + 12 * UNIT
            else:
                box(xpos, ypos, width * UNIT, 39, fill)
                center = xpos + width * UNIT / 2
            value = str(VALUES[row][column])
            if column >= 3:
                value += ' · 16 B'
            text(center, ypos + 20, value, 12, ha='center',
                 color=C['orange'] if column == 1 and not destination else C['ink'])
        end = start + len(rows) * width * UNIT
        text(end + 12, ypos + 20, f'{width} B/cell', 9, color=C['muted'])


def step(x, y):
    box(x, y, 560, 80, C['orange_fill'], '#dfc39b', rounded=True)
    text(x + 280, y + 24, 'FILTER  ·  selector < 50', 16, weight='bold', ha='center', color=C['orange'])
    text(x + 280, y + 54, '48 ✓     85 ×     22 ✓     59 ×', 13, ha='center')
    arrow((x + 280, y + 81), (x + 280, y + 115), C['orange'])
    text(x + 280, y + 135, 'Matched indices in this excerpt: [4, 6]', 13, ha='center', weight='bold')


text(60, 47, 'Filtering and deep-copying five-column records', 24, weight='bold')
text(60, 87, '16-byte text payloads + cell metadata  •  Four example rows from the million-row source  •  Same predicate and values', 13, color=C['muted'])
ax.plot([1000, 1000], [125, 1110], color=C['line'], linewidth=1)

text(60, 142, 'GD  ·  Row layout / AoS', 19, weight='bold')
text(1060, 142, 'gd-rs with constant size fields  ·  SoA', 19, weight='bold')
text(60, 178, 'SOURCE  ·  one contiguous row buffer, wrapped below by row', 11, color=C['muted'])
text(1060, 178, 'SOURCE  ·  five separately allocated column buffers', 11, color=C['muted'])
gd_start = gd_table(60, 231, [4, 5, 6, 7])
soa_table(1060, 224, [4, 5, 6, 7])
text(130, 430, 'id → selector → amount → name → message → next row …', 11, color=C['muted'])
text(130, 456, 'Selector values are 72 bytes apart.', 12, weight='bold', color=C['orange'])
text(1225, 456, 'Selector values are adjacent: 8 bytes apart.', 12, weight='bold', color=C['orange'])

# Reading the selector, rather than passing whole source records to the filter.
ax.plot([gd_start + 12 * UNIT, gd_start + 12 * UNIT, 105, 105, 440],
        [401, 411, 411, 483, 483], color=C['orange'], linewidth=1.7)
arrow((440, 483), (440, 496), C['orange'])
ax.plot([1225, 1204, 1204, 1440], [286, 286, 483, 483], color=C['orange'], linewidth=1.7)
arrow((1440, 483), (1440, 496), C['orange'])
step(160, 499)
step(1160, 499)
arrow((440, 650), (440, 690))
arrow((1440, 650), (1440, 690))

box(145, 702, 590, 89, C['blue_fill'], '#b6cddd', rounded=True)
text(440, 726, 'COPY  ·  memcpy one complete row per match', 14, weight='bold', ha='center')
text(440, 756, '72 bytes → 72 bytes', 14, ha='center')
box(1145, 702, 590, 89, C['blue_fill'], '#b6cddd', rounded=True)
text(1440, 726, 'COPY  ·  gather all five cells per match', 14, weight='bold', ha='center')
text(1440, 756, '3 × u64 + 2 × (u32 + [u8;16]) = 64 bytes', 13, ha='center')
arrow((440, 797), (440, 830))
arrow((1440, 797), (1440, 830))

text(60, 853, 'DESTINATION  ·  one new row buffer (excerpt)', 13, weight='bold')
text(1060, 853, 'DESTINATION  ·  five new column buffers (excerpt)', 13, weight='bold')
gd_table(60, 914, [4, 6], destination=True)
soa_table(1060, 901, [4, 6], destination=True)
text(130, 1025, 'Earlier matches omitted; this excerpt fills target rows 3–4.', 11, color=C['muted'])
text(130, 1053, 'Every matched row carries its string metadata too.', 11, color=C['muted'])

# Byte-layout detail makes the difference in string-cell width explicit.
ax.plot([60, 1940], [1140, 1140], color=C['line'], linewidth=1)
text(60, 1158, 'Inside one text cell', 15, weight='bold')
parts = [(4, 'Length\n4 B', C['meta']), (16, 'Text bytes\n16 B', C['blue_fill']),
         (4, 'NUL + tail\n4 B', C['meta'])]
cursor = 230
for width, label, fill in parts:
    box(cursor, 1178, width * 21, 57, fill, C['meta_line'] if width == 4 else C['line'], '///' if width == 4 else None)
    text(cursor + width * 21 / 2, 1207, label, 9 if width == 4 else 12, ha='center')
    cursor += width * 21
text(cursor + 18, 1207, 'GD: 24 B', 12, weight='bold')
text(1060, 1207, 'gd-rs', 12, weight='bold')
box(1140, 1178, 4 * 21, 57, C['meta'], C['meta_line'], '///')
text(1182, 1207, 'Meta\n4 B', 9, ha='center')
box(1224, 1178, 16 * 21, 57, C['blue_fill'])
text(1392, 1207, '[u8;16] · 16 text bytes', 12, ha='center')
text(1590, 1188, 'gd-rs: 20 B', 12, weight='bold')
text(1590, 1211, 'u32 = tag (8 bits) + valid length (24 bits)', 10, color=C['muted'])
text(1590, 1233, 'Both metadata words are deep-copied.', 10, color=C['muted'])

box(60, 1260, 22, 22, C['blue_fill'])
text(94, 1271, 'Selected cells / copied payload', 11)
box(610, 1260, 22, 22, C['orange_fill'])
text(644, 1271, 'Selector read by the filter', 11)
box(1110, 1260, 22, 22, C['meta'], C['meta_line'], '///')
text(1144, 1271, 'Cell metadata / GD tail bytes', 11)
text(60, 1322, 'Deep copy: the destination owns all its bytes.  n4 / m4 etc. abbreviate the 16-byte name and message payloads.', 11, color=C['muted'])
text(60, 1354, 'With 128-byte text: GD = 296 B per row; gd-rs = 288 B across five columns, including 8 B of metadata. gd-rs uses constant size fields in this comparison.', 11, color=C['muted'])

fig.savefig(OUT / 'gd-rust-memory-layout.png', dpi=150)
svg_path = OUT / 'gd-rust-memory-layout.svg'
fig.savefig(svg_path)
svg_path.write_text('\n'.join(line.rstrip() for line in svg_path.read_text().splitlines()) + '\n')
plt.close(fig)
print(OUT / 'gd-rust-memory-layout.png')
