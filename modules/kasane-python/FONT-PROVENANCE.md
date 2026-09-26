# Inspection font provenance

O2 numeric object labels use the small 3×5 bitmap digits defined in
`python/kasane/_inspection.py`; no external font file is distributed.

O3 contact-sheet headers use `PIL.ImageFont.load_default()` from the pinned
optional dependency Pillow 12.3.0. Pillow documents this as its bundled
Aileron Regular subset when FreeType is available, with a built-in fallback
otherwise. Pillow carries its [MIT-CMU license](https://github.com/python-pillow/Pillow/blob/12.3.0/LICENSE),
and the [Aileron project](https://github.com/Nicholaiii/Aileron) lists the SIL
Open Font License. Kasane does not copy or redistribute a separate font file.
The report records the Pillow version and substitutes `?` for non-ASCII
characters in sheet headers; original Unicode names remain in JSON.
