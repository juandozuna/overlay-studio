#!/usr/bin/env python3
"""Receive an Overlay Studio frame through the installed NDI runtime."""
import ctypes as c
import os
import time
import argparse

parser = argparse.ArgumentParser()
parser.add_argument('--source', default='Overlay Studio', help='Unique substring of the sender name')
parser.add_argument('--seconds', type=float, default=0, help='Measure continuous reception for up to 60 seconds')
parser.add_argument('--alpha', choices=['visible', 'hidden'], default='visible')
parser.add_argument('--sample', help='Inspect a canvas pixel as x,y')
parser.add_argument('--expect-sample-alpha', type=int, help='Wait for the sampled pixel to have this alpha')
args = parser.parse_args()
if not 0 <= args.seconds <= 60:
    parser.error('--seconds must be between 0 and 60')
sample = None
if args.sample:
    try:
        sample = tuple(int(v) for v in args.sample.split(','))
        if len(sample) != 2 or min(sample) < 0:
            raise ValueError
    except ValueError:
        parser.error('--sample must be nonnegative x,y coordinates')

class Source(c.Structure):
    _fields_ = [("name", c.c_char_p), ("url", c.c_char_p)]
class Find(c.Structure):
    _fields_ = [("local", c.c_bool), ("groups", c.c_char_p), ("ips", c.c_char_p)]
class Receive(c.Structure):
    _fields_ = [("source", Source), ("color", c.c_int), ("bandwidth", c.c_int), ("fields", c.c_bool), ("name", c.c_char_p)]
class Video(c.Structure):
    _fields_ = [("width", c.c_int), ("height", c.c_int), ("fourcc", c.c_uint), ("fps_n", c.c_int), ("fps_d", c.c_int), ("aspect", c.c_float), ("format", c.c_int), ("timecode", c.c_int64), ("data", c.c_void_p), ("stride", c.c_int), ("metadata", c.c_char_p), ("timestamp", c.c_int64)]

paths = [os.environ.get("OVERLAY_NDI_LIBRARY", ""), "/usr/local/lib/libndi.dylib", "/Applications/Resolume Arena/libndi.dylib"]
ndi = None
for path in paths:
    if not path:
        continue
    try:
        ndi = c.CDLL(path)
        break
    except OSError:
        pass
if ndi is None:
    raise SystemExit("NDI runtime not found")
def bind(name, result, args):
    function = getattr(ndi, name)
    function.restype, function.argtypes = result, args
    return function
initialize = bind("NDIlib_initialize", c.c_bool, [])
find_create = bind("NDIlib_find_create_v2", c.c_void_p, [c.POINTER(Find)])
find_sources = bind("NDIlib_find_get_current_sources", c.POINTER(Source), [c.c_void_p, c.POINTER(c.c_uint)])
find_destroy = bind("NDIlib_find_destroy", None, [c.c_void_p])
recv_create = bind("NDIlib_recv_create_v3", c.c_void_p, [c.POINTER(Receive)])
recv_capture = bind("NDIlib_recv_capture_v2", c.c_int, [c.c_void_p, c.POINTER(Video), c.c_void_p, c.c_void_p, c.c_uint])
recv_free = bind("NDIlib_recv_free_video_v2", None, [c.c_void_p, c.POINTER(Video)])
recv_destroy = bind("NDIlib_recv_destroy", None, [c.c_void_p])
if not initialize():
    raise SystemExit("NDI initialization failed")
finder = find_create(c.byref(Find(True, b"Public", b"127.0.0.1")))
if not finder:
    raise SystemExit("NDI finder could not be created")
receiver = None
try:
    source = None
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        count = c.c_uint()
        sources = find_sources(finder, c.byref(count))
        for i in range(count.value):
            name = sources[i].name or b""
            if args.source.encode() in name:
                source = Source(bytes(name), bytes(sources[i].url or b""))
                break
        if source:
            break
        time.sleep(.1)
    if source is None:
        raise SystemExit(f"Source matching {args.source!r} not discoverable, including through loopback. Check that the NDI output is started.")
    print("Discovered:", source.name.decode(errors="replace"), source.url.decode(errors="replace"))
    receiver = recv_create(c.byref(Receive(source, 0, 100, False, b"Overlay Studio validation")))
    if not receiver:
        raise SystemExit("NDI receiver could not be created")
    deadline = time.monotonic() + 10
    first_frame = None
    received = 0
    gaps = []
    previous_frame = None
    while time.monotonic() < deadline:
        frame = Video()
        kind = recv_capture(receiver, c.byref(frame), None, None, 500)
        if kind != 1:
            continue
        try:
            code = frame.fourcc.to_bytes(4, "little")
            if code != b"BGRA":
                raise SystemExit(f"Expected BGRA alpha frame; received {code!r}")
            data = c.string_at(frame.data, frame.stride * frame.height)
            alpha = [data[y * frame.stride + x * 4 + 3] for y in range(0, frame.height, 8) for x in range(0, frame.width, 8)]
            expected = max(alpha) == 0 if args.alpha == 'hidden' else min(alpha) == 0 and max(alpha) >= 200
            sampled = None
            if sample:
                x, y = sample
                if x >= frame.width or y >= frame.height:
                    raise SystemExit('Sample lies outside the received frame')
                offset = y * frame.stride + x * 4
                sampled = tuple(data[offset:offset+4])
                if args.expect_sample_alpha is not None:
                    expected = expected and sampled[3] == args.expect_sample_alpha
            if not expected:
                if first_frame is not None:
                    raise SystemExit('Alpha changed unexpectedly during continuous reception')
                continue
            now = time.monotonic()
            if first_frame is None:
                first_frame = now
                deadline = now + max(args.seconds, .1) + 10
                print(f"Received {frame.width} × {frame.height}, {frame.fps_n}/{frame.fps_d} fps, BGRA; alpha range {min(alpha)}–{max(alpha)}")
                if sampled is not None:
                    print(f"Pixel {sample}: BGRA {sampled}")
            received += 1
            if previous_frame is not None:
                gaps.append(now - previous_frame)
            previous_frame = now
            if now - first_frame >= args.seconds:
                if args.seconds:
                    print(f"Continuous reception: {received} frames in {now-first_frame:.2f}s, {(received-1)/(now-first_frame):.1f} fps; maximum capture gap {max(gaps, default=0)*1000:.1f}ms")
                break
        finally:
            recv_free(receiver, c.byref(frame))
    else:
        raise SystemExit("No video received within ten seconds")
finally:
    if receiver:
        recv_destroy(receiver)
    find_destroy(finder)
