import base64, hmac, hashlib, struct, time, sys
k = base64.b32decode(sys.argv[1] if len(sys.argv) > 1 else "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP")
c = struct.pack(">Q", int(time.time()) // 30)
h = hmac.new(k, c, hashlib.sha1).digest()
o = h[-1] & 15
print("%06d" % ((struct.unpack(">I", h[o:o+4])[0] & 0x7fffffff) % 1000000))
