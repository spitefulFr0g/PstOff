"""Runs inside the libpff sandbox. Prints the folder tree with message counts."""
import sys
import pypff


def walk(folder, depth):
    try:
        associated = folder.number_of_associated_sub_messages
    except Exception:
        associated = '?'
    print('  ' * depth + f'{folder.name!r} id={folder.identifier:x} msgs={folder.number_of_sub_messages} assoc={associated}')
    for i in range(folder.number_of_sub_folders):
        walk(folder.get_sub_folder(i), depth + 1)


f = pypff.file()
f.open(sys.argv[1])
walk(f.get_root_folder(), 0)
