"""Translate Word UTF-16 offsets without changing the captured text."""


class InvalidSourceOffset(ValueError):
    pass


def utf16_length(text: str) -> int:
    return sum(2 if ord(ch) > 0xFFFF else 1 for ch in text)


class SourceText:
    def __init__(self, text: str):
        self.text = text
        self.boundaries = {0: 0}
        offset = 0
        for index, ch in enumerate(text):
            if 0xD800 <= ord(ch) <= 0xDFFF:
                raise InvalidSourceOffset("UNPAIRED_SURROGATE: captured text is not Unicode scalar text")
            offset += 2 if ord(ch) > 0xFFFF else 1
            self.boundaries[offset] = index + 1
        self.length = offset

    def index(self, offset: int) -> int:
        try:
            return self.boundaries[offset]
        except KeyError:
            raise InvalidSourceOffset(
                "INVALID_UTF16_BOUNDARY: offset %s is outside the text or splits a surrogate pair"
                % offset
            ) from None

    def slice(self, start: int, end: int) -> str:
        if start > end:
            raise InvalidSourceOffset("REVERSED_SOURCE_RANGE: %s > %s" % (start, end))
        return self.text[self.index(start):self.index(end)]
