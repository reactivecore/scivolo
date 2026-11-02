"""The TASTy quotes in NIR dumps, as the string literals of `unpickleExprV2` calls hold them,
compared without their positions."""
import base64
import hashlib
import re

QUOTE = re.compile(r'"(XKGr[A-Za-z0-9+/=]*)"')
TASTY_MAGIC = bytes([0x5C, 0xA1, 0xAB, 0x1F])
POSITIONS_SOURCE = 4


def nat(data, i):
    """The TASTy natural number at `data[i]` and the index after it."""
    x = 0
    while True:
        b = data[i]
        i += 1
        x = x << 7 | b & 0x7F
        if b & 0x80:
            return x, i


def long_int(data, i):
    """The TASTy signed number at `data[i]` and the index after it."""
    b = data[i]
    x = (b & 0x7F) - (0x80 if b & 0x40 else 0)
    while not b & 0x80:
        i += 1
        b = data[i]
        x = x << 7 | b & 0x7F
    return x, i + 1


def source_names(positions):
    """The name indices the Positions section `positions` refers to for its sources."""
    lines, i = nat(positions, 0)
    for _ in range(lines):
        _, i = nat(positions, i)
    found = set()
    while i < len(positions):
        header, i = long_int(positions, i)
        if header == POSITIONS_SOURCE:
            ref, i = nat(positions, i)
            found.add(ref)
            continue
        for bit in (4, 2, 1):
            if header & bit:
                _, i = long_int(positions, i)
    return found


def quote_key(text):
    """A key of the TASTy quote Base64 `text` without its header's UUID and its positions: its
    names but those only positions use, and its ASTs; None if `text` is no TASTy."""
    try:
        data = base64.b64decode(text, validate=True)
        if data[:4] != TASTY_MAGIC:
            return None
        i = 4
        for _ in range(3):
            _, i = nat(data, i)
        n, i = nat(data, i)
        i += n + 16
        size, i = nat(data, i)
        end, names = i + size, []
        while i < end:
            n, start = nat(data, i + 1)
            names.append((data[i:start + n], data[start:start + n]))
            i = start + n
        sections = {}
        while i < len(data):
            ref, i = nat(data, i)
            n, i = nat(data, i)
            sections[names[ref][1].decode(errors="replace")] = (ref, data[i:i + n])
            i += n
        dropped = {r for k, (r, _) in sections.items() if k != "ASTs"}
        if "Positions" in sections:
            dropped |= source_names(sections["Positions"][1])
        kept = b"".join(entry for k, (entry, _) in enumerate(names) if k not in dropped)
        return hashlib.sha1(kept + b"|" + sections.get("ASTs", (0, b""))[1]).hexdigest()[:16]
    except (IndexError, ValueError, KeyError):
        return None


def without_positions(lines):
    """`lines` with each string literal of a TASTy quote replaced by its `quote_key`."""
    sub = lambda m: f'"<quote {k}>"' if (k := quote_key(m[1])) else m[0]
    return [QUOTE.sub(sub, l) for l in lines]
