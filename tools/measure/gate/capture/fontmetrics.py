import struct, sys, os, glob
def tables(data, off=0):
    tag = data[off:off+4]
    if tag == b'ttcf':
        n = struct.unpack('>I', data[off+8:off+12])[0]
        offs = struct.unpack('>%dI' % n, data[off+12:off+12+4*n])
        return [tables(data, o)[0] for o in offs]
    num = struct.unpack('>H', data[off+4:off+6])[0]
    t = {}
    for i in range(num):
        e = off + 12 + 16*i
        tg, cs, o, ln = struct.unpack('>4sIII', data[e:e+16])
        t[tg.decode('latin1')] = (o, ln)
    return [t]
def metrics(path, idx=0):
    data = open(path, 'rb').read()
    t = tables(data)[idx]
    upem = struct.unpack('>H', data[t['head'][0]+18:t['head'][0]+20])[0]
    ho = t['hhea'][0]
    asc, desc, gap = struct.unpack('>hhh', data[ho+4:ho+10])
    oo = t['OS/2'][0]
    fssel = struct.unpack('>H', data[oo+62:oo+64])[0]
    ta, td, tg, wa, wd = struct.unpack('>hhhHH', data[oo+68:oo+78])
    name = os.path.basename(path) + (f'#{idx}' if idx else '')
    return name, upem, (asc, desc, gap), (ta, td, tg), (wa, wd), (fssel >> 7) & 1
if __name__ == '__main__':
    for p in sys.argv[1:]:
        p, _, i = p.partition('#')
        n, u, h, ty, w, ut = metrics(p, int(i or 0))
        print(f"{n:34s} upem={u:5d} hhea={h[0]}/{h[1]}/{h[2]} ({(h[0]-h[1]+h[2])/u:.4f}em)  win={w[0]}/{w[1]} ({(w[0]+w[1])/u:.4f}em)  typo={ty[0]}/{ty[1]}/{ty[2]} ({(ty[0]-ty[1]+ty[2])/u:.4f}em) useTypo={ut}")
