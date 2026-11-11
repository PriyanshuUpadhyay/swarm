"""V2 fault peer. Framing and faults are HAND values, independent of a candidate."""
import json
import os
from pathlib import Path
import struct
import sys

def read():
    prefix = sys.stdin.buffer.read(4)
    if not prefix:
        return None
    size = struct.unpack('>I', prefix)[0]
    return json.loads(sys.stdin.buffer.read(size))

def send(value):
    data = json.dumps(value, separators=(',', ':')).encode()
    sys.stdout.buffer.write(struct.pack('>I', len(data)) + data)
    sys.stdout.buffer.flush()

mode = sys.argv[1]
Path(sys.argv[2], mode + ".pid").write_text(str(os.getpid()))
hello = read()
assert hello['op'] == 'hello' and hello['protocol'] == 2
send({'op': 'ready', 'protocol': 1 if mode == 'incompatible' else 2, 'formats': ['claude']})
if mode == 'incompatible':
    sys.stdin.buffer.read()
    sys.exit(0)
opened = read()
assert opened['op'] == 'open'
binding = opened['binding']
send({'op': 'opened', 'request_id': opened['request_id'], 'binding': binding})
page = read()
assert page['op'] == 'page'
tag = {'binding': binding.copy(), 'request_id': page['request_id'], 'transport_id': 1}
if mode == 'stale':
    tag['binding']['generation'] += 1
send({'op': 'begin', 'tag': tag})
if mode == 'saved-failure':
    # Incomplete saved publication fails one job; the same authority remains usable.
    send({'op': 'fault', 'request_id': page['request_id'], 'binding': binding, 'code': 'core'})
    again = read()
    assert again['op'] == 'page'
    tag = {'binding': binding, 'request_id': again['request_id'], 'transport_id': 2}
    data = open(sys.argv[3], 'rb').read()
    send({'op': 'begin', 'tag': tag})
    send({'op': 'part', 'tag': tag, 'part_index': 0, 'bytes': list(data)})
    send({'op': 'commit', 'tag': tag, 'part_count': 1, 'payload_bytes': len(data)})
else:
    if mode == 'cancel':
        cancel = read()
        assert cancel['op'] == 'cancel' and cancel['target_request_id'] == page['request_id']
    # This Commit is stale by binding or cancellation. It must not publish empty rows.
    send({'op': 'commit', 'tag': tag, 'part_count': 0, 'payload_bytes': 0})
    if mode == 'cancel':
        send({'op': 'fenced', 'tag': tag})
        send({'op': 'cancelled', 'request_id': cancel['request_id'], 'binding': binding,
              'target_request_id': page['request_id'], 'transport_id': 1, 'exit': {'kind': 'code', 'code': 0}})
closed = read()
if closed is not None:
    assert closed['op'] == 'close'
    send({'op': 'closed', 'request_id': closed['request_id'], 'binding': binding})
