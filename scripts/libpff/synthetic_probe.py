"""Runs only under libpff-check.py's bwrap. No property values leave the guest."""
import hashlib
import json
import sys
import ctypes as c

import pypff

errors = []


def properties(item):
    result = {}
    for i in range(item.number_of_record_sets):
        row = item.get_record_set(i)
        for j in range(row.number_of_entries):
            entry = row.get_entry(j)
            raw = entry.data
            if raw is None:
                raw = b''
            result[f'{entry.entry_type:04x}:{entry.value_type:04x}'] = {
                'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
            }
    return result


def records(item):
    result = {'id': item.identifier}
    try:
        result['properties'] = properties(item)
    except Exception as error:
        errors.append({'id': item.identifier, 'phase': 'properties', 'error': type(error).__name__})
    return result


def message(item):
    result = records(item)
    try:
        result['subitems'] = [records(item.get_sub_item(i)) for i in range(item.number_of_sub_items)]
        result['attachments'] = []
        for i in range(item.number_of_attachments):
            attachment = item.get_attachment(i)
            a = records(attachment)
            # Read the attachment payload through libpff's stream interface as well.
            if '3701:0102' in a.get('properties', {}):
                payload = attachment.read_buffer(attachment.get_size())
                a['payload'] = {'size': len(payload), 'sha256': hashlib.sha256(payload).hexdigest()}
            a['subitems'] = [records(attachment.get_sub_item(j)) for j in range(attachment.number_of_sub_items)]
            result['attachments'].append(a)
    except Exception as error:
        errors.append({'id': item.identifier, 'phase': 'content', 'error': type(error).__name__})
    return result


def folder(item):
    result = records(item)
    result['folders'], result['items'] = [], []
    for kind, count, getter, reader in [
        ('folders', item.number_of_sub_folders, item.get_sub_folder, folder),
        ('items', item.number_of_sub_messages, item.get_sub_message, message),
    ]:
        for i in range(count):
            try:
                child = getter(i)
            except Exception as error:
                errors.append({'parent': item.identifier, 'index': i,
                               'phase': kind + '-open', 'error': type(error).__name__})
                continue
            result[kind].append(reader(child))
    return result


def native_details(path, tree):
    """20180714's Python binding omits recipients and attached-item access.

    Use the same sandboxed library's documented C API for those, and cached
    contents-table cells. Never expose the bytes; only their lengths and hashes.
    """
    lib = c.CDLL('libpff.so.1')
    p, pp = c.c_void_p, c.POINTER(c.c_void_p)
    u32, size = c.c_uint32, c.c_size_t
    signatures = {
        'file_initialize': [pp, pp], 'file_open': [p, c.c_char_p, c.c_int, pp],
        'file_close': [p, pp], 'file_free': [pp, pp], 'item_free': [pp, pp],
        'file_get_item_by_identifier': [p, u32, pp, pp],
        'message_get_recipients': [p, pp, pp],
        'message_get_attachment': [p, c.c_int, pp, pp],
        'attachment_get_item': [p, pp, pp],
        'item_get_number_of_record_sets': [p, c.POINTER(c.c_int), pp],
        'item_get_entry_value': [p, c.c_int, u32, c.POINTER(u32), pp, c.POINTER(size), c.c_uint8, pp],
    }
    for name, args in signatures.items():
        fn = getattr(lib, 'libpff_' + name)
        fn.argtypes, fn.restype = args, c.c_int

    def call(name, *args):
        error = p()
        result = getattr(lib, 'libpff_' + name)(*args, c.byref(error))
        if error:
            lib.libpff_error_free.argtypes = [pp]
            lib.libpff_error_free(c.byref(error))
        if result < 0:
            raise OSError(name)
        return result

    def values(item, tags):
        count = c.c_int()
        call('item_get_number_of_record_sets', item, c.byref(count))
        result = []
        for row in range(count.value):
            cells = {}
            for tag in tags:
                kind = u32(0x1f if tag in (0x3001, 0x3003, 0x001a) else 0x40 if tag == 0x3008 else 0x102 if tag in (2, 3, 4, 0x65e2, 0x65e3) else 3)
                data, length = p(), size()
                if call('item_get_entry_value', item, row, tag, c.byref(kind), c.byref(data), c.byref(length), 0):
                    raw = c.string_at(data, length.value)
                    cells[f'{tag:04x}:{kind.value:04x}'] = {'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
            result.append(cells)
        return result

    def get_item(file, nid):
        item = p()
        if call('file_get_item_by_identifier', file, nid, c.byref(item)) != 1:
            raise OSError('missing item')
        return item

    file = p()
    call('file_initialize', c.byref(file))
    call('file_open', file, path.encode(), 1)
    result = {}
    try:
        nameid = get_item(file, 0x61)
        try:
            result['nameid'] = values(nameid, [2, 3])
        finally:
            call('item_free', c.byref(nameid))
        def walk(node):
            table = get_item(file, (node['id'] & ~31) | 0xe)
            try:
                result[str(node['id'])] = {'cached': values(table, [0x67f2, 0x3008, 0x65e2, 0x65e3])}
            finally:
                call('item_free', c.byref(table))
            for info in node['items']:
                message = get_item(file, info['id'])
                detail = {}
                try:
                    recipient = p()
                    if call('message_get_recipients', message, c.byref(recipient)):
                        try:
                            detail['recipients'] = values(recipient, [0x0c15, 0x3001, 0x3003])
                        finally:
                            call('item_free', c.byref(recipient))
                    for i, a in enumerate(info.get('attachments', [])):
                        if '3701:000d' in a.get('properties', {}):
                            attachment, embedded = p(), p()
                            call('message_get_attachment', message, i, c.byref(attachment))
                            try:
                                if call('attachment_get_item', attachment, c.byref(embedded)) != 1:
                                    raise OSError('missing embedded item')
                                detail['embedded'] = values(embedded, [0x001a, 0x0e08])
                            finally:
                                if embedded: call('item_free', c.byref(embedded))
                                call('item_free', c.byref(attachment))
                    result[str(info['id'])] = detail
                finally:
                    call('item_free', c.byref(message))
            for child in node['folders']:
                walk(child)
        walk(tree)
    finally:
        call('file_close', file)
        call('file_free', c.byref(file))
    return result


f = pypff.file()
f.open(sys.argv[1])
tree = folder(f.get_root_folder())
native = {}
if not errors:
    try:
        native = native_details(sys.argv[1], tree)
    except Exception as error:
        errors.append({'phase': 'native-content', 'error': type(error).__name__, 'operation': str(error)})
print(json.dumps({'reader_version': pypff.get_version(), 'tree': tree, 'native': native, 'errors': errors}, sort_keys=True))
f.close()
