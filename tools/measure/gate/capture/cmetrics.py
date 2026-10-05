import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fontmetrics import tables
def load(path):
    d = open(path, 'rb').read(); t = tables(d)[0]
    upem = struct.unpack('>H', d[t['head'][0]+18:t['head'][0]+20])[0]
    nh = struct.unpack('>H', d[t['hhea'][0]+34:t['hhea'][0]+36])[0]
    ho = t['hmtx'][0]
    adv = [struct.unpack('>H', d[ho+4*i:ho+4*i+2])[0] for i in range(nh)]
    co = t['cmap'][0]; n = struct.unpack('>H', d[co+2:co+4])[0]; cmap = {}
    for i in range(n):
        pid, eid, off = struct.unpack('>HHI', d[co+4+8*i:co+12+8*i])
        so = co+off
        if struct.unpack('>H', d[so:so+2])[0] == 4 and pid == 3 and eid == 1:
            segx2 = struct.unpack('>H', d[so+6:so+8])[0]; s = segx2//2
            ends = struct.unpack(f'>{s}H', d[so+14:so+14+segx2])
            starts = struct.unpack(f'>{s}H', d[so+16+segx2:so+16+2*segx2])
            deltas = struct.unpack(f'>{s}h', d[so+16+2*segx2:so+16+3*segx2])
            ro = so+16+3*segx2; ros = struct.unpack(f'>{s}H', d[ro:ro+segx2])
            for k in range(s):
                for c in range(starts[k], ends[k]+1):
                    if c == 0xFFFF: continue
                    if ros[k] == 0: g = (c + deltas[k]) & 0xFFFF
                    else:
                        a = ro + 2*k + ros[k] + 2*(c - starts[k]); g = struct.unpack('>H', d[a:a+2])[0]
                        if g: g = (g + deltas[k]) & 0xFFFF
                    cmap[c] = g
    return upem, adv, cmap
def width(text, size_pt, f):
    upem, adv, cmap = f
    return sum(adv[min(cmap.get(ord(c), 0), len(adv)-1)] for c in text) * size_pt * 20 / upem
if __name__ == '__main__':
    f = load(sys.argv[1])
    for ch in "0 ilntmMAV.":
        print(repr(ch), f[1][min(f[2].get(ord(ch),0), len(f[1])-1)], round(width(ch, 12, f), 3))
