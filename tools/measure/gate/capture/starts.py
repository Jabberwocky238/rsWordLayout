import re, sys, collections
# Print the FormatLine (ENTER) start sequence per available width from a probe log.
for path in sys.argv[1:]:
    seqs = collections.OrderedDict()
    for line in open(path, errors="replace"):
        m = re.search(r"ENTER n=\d+ tid=\d+ w2=([0-9a-f]+) w3=([0-9a-f]+)", line)
        if m:
            seqs.setdefault(int(m.group(2), 16), []).append(int(m.group(1), 16))
    print(path.split("/")[-1], {w: len(s) for w, s in seqs.items()})
    for w, s in seqs.items():
        if w > 1000:
            print(f"  w3={w}: {s[:60]}")
