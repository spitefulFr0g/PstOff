"""Runs inside the libpff sandbox. Prints every reachable property as a hash, never its value."""
import hashlib, json, sys
import pypff

records, errors, nodes, visited = {}, [], {}, set()


def walk(item):
    try:
        nid = int(item.identifier)
    except Exception as e:
        errors.append({'phase': 'identifier', 'error': type(e).__name__})
        return
    if nid in visited:
        return
    visited.add(nid)
    nodes[str(nid)] = type(item).__name__
    try:
        for si in range(item.number_of_record_sets):
            row = item.get_record_set(si)
            for ei in range(row.number_of_entries):
                entry = row.get_entry(ei)
                key = f'{nid:08x}:{si}:{entry.entry_type:04x}:{entry.value_type:04x}'
                try:
                    raw = entry.data
                    if raw is None:
                        value = {'is_none': True}
                    elif isinstance(raw, (bytes, bytearray, memoryview)):
                        value = {'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
                    else:
                        raise TypeError('not raw bytes')
                    if key in records:
                        errors.append({'key': key, 'phase': 'duplicate-key'})
                    records[key] = value
                except Exception as e:
                    errors.append({'key': key, 'phase': 'raw-data', 'error': type(e).__name__})
    except Exception as e:
        errors.append({'nid': nid, 'phase': 'records', 'error': type(e).__name__})
    try:
        for i in range(item.number_of_sub_items):
            walk(item.get_sub_item(i))
    except Exception as e:
        errors.append({'nid': nid, 'phase': 'subitems', 'error': type(e).__name__})
    if hasattr(item, 'number_of_attachments'):
        try:
            for i in range(item.number_of_attachments):
                walk(item.get_attachment(i))
        except Exception as e:
            errors.append({'nid': nid, 'phase': 'attachments', 'error': type(e).__name__})
    for name in ('get_recipients', 'get_embedded_message'):
        if hasattr(item, name):
            try:
                child = getattr(item, name)()
                if child is not None:
                    walk(child)
            except Exception as e:
                errors.append({'nid': nid, 'phase': name, 'error': type(e).__name__})


f = pypff.file()
f.open(sys.argv[1])
walk(f.get_message_store())
walk(f.get_root_folder())
print(json.dumps({'reader_version': pypff.get_version(), 'nodes': nodes, 'records': records, 'errors': errors}, sort_keys=True))
f.close()
