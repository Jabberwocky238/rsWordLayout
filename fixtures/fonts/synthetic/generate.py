# Synthetic outlines and metadata authored for this repository, MIT OR Apache-2.0.
# Run: uv run --with fonttools==4.59.0 python fixtures/fonts/generate.py
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTCollection

ROOT = Path(__file__).parent

def make(family, weight, italic, codes):
    names = ['.notdef'] + [f'u{cp:04X}' for cp in codes]
    fb = FontBuilder(1000, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap(dict(zip(codes, names[1:])))
    glyphs = {}
    for name in names:
        pen = TTGlyphPen(None)
        pen.moveTo((50, 0)); pen.lineTo((450, 0)); pen.lineTo((250, 700)); pen.closePath()
        glyphs[name] = pen.glyph()
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({n: (500, 50) for n in names})
    fb.setupHorizontalHeader(ascent=800, descent=-200)
    fb.setupNameTable({'familyName': family, 'styleName': 'Italic' if italic else 'Regular',
                      'uniqueFontIdentifier': f'{family}-{weight}-{italic}',
                      'fullName': family, 'psName': family.replace(' ', '')})
    fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800,
                usWinDescent=200, usWeightClass=weight, fsSelection=1 if italic else 64)
    fb.setupPost(); fb.setupMaxp()
    fb.font['head'].created = 3400000000
    fb.font['head'].modified = 3400000000
    fb.font['head'].macStyle = 2 if italic else 0
    fb.font.recalcTimestamp = False
    return fb.font

fonts = [make('Kernel Sans', 400, False, [0x41, 0x42, 0x20]),
         make('Kernel Sans', 700, True, [0x41, 0x42, 0x20]),
         make('Hidden Fallback', 400, False, [0x4E00, 0xF0FC])]
for font, name in zip(fonts, ['regular.ttf', 'italic.ttf', 'fallback.ttf']):
    font.save(ROOT / name)
collection = TTCollection()
collection.fonts = fonts[:2]
collection.save(ROOT / 'two-faces.ttc')
