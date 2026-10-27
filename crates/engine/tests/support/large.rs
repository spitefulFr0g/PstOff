//! Deterministic file-backed v36 corpus. Payload blocks are written immediately.
//! Generator metadata is outside the converter process and its measured peak.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

// Independent table CRC for the large builder; do not call either reader's checksum.
const CRC: [u32; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ if value & 1 == 0 { 0 } else { 0xedb88320 };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
};
fn crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |sum, byte| {
        (sum >> 8) ^ CRC[((sum as u8) ^ byte) as usize]
    })
}

pub(super) struct Disk {
    file: File,
    bbt: File,
    directory: PathBuf,
    next: u64,
}
impl Disk {
    fn new(path: &Path) -> Self {
        let directory = path.with_extension("builder");
        std::fs::create_dir(&directory).unwrap();
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        file.set_len(4096).unwrap();
        Self {
            file,
            bbt: File::create(directory.join("bbt")).unwrap(),
            directory,
            next: 3,
        }
    }
    pub(super) fn block(&mut self, bytes: &[u8], internal: bool) -> u64 {
        let id = (self.next * 4) | if internal { 2 } else { 0 };
        self.next += 1;
        let at = self.file.seek(SeekFrom::End(0)).unwrap();
        let padded = (bytes.len() + 24).div_ceil(512) * 512;
        let mut block = vec![0; padded];
        block[..bytes.len()].copy_from_slice(bytes);
        let t = padded - 24;
        p16(&mut block, t, bytes.len() as u16);
        p16(&mut block, t + 2, signature(id, at as usize));
        p32(&mut block, t + 4, crc(bytes));
        p64(&mut block, t + 8, id);
        p16(&mut block, t + 18, bytes.len() as u16);
        self.file.write_all(&block).unwrap();
        let mut entry = [0; 24];
        p64(&mut entry, 0, id);
        p64(&mut entry, 8, at);
        p16(&mut entry, 16, bytes.len() as u16);
        p16(&mut entry, 18, 1);
        self.bbt.write_all(&entry).unwrap();
        id
    }
    fn tree(&mut self, input: &Path, mut width: usize, kind: u8) -> (u64, u64) {
        let mut input = input.to_owned();
        let mut level = 0;
        loop {
            let mut reader = File::open(&input).unwrap();
            let count = reader.metadata().unwrap().len() / width as u64;
            assert!(count > 0);
            let parent = self.directory.join(format!("{kind}-{level}"));
            let mut parents = File::create(&parent).unwrap();
            let mut left = count;
            let mut last = (0, 0);
            while left > 0 {
                let n = left.min((4056 / width) as u64) as usize;
                let mut entries = vec![0; n * width];
                reader.read_exact(&mut entries).unwrap();
                let key = u64::from_le_bytes(entries[..8].try_into().unwrap());
                let at = self.file.seek(SeekFrom::End(0)).unwrap().div_ceil(4096) * 4096;
                self.file.seek(SeekFrom::Start(at)).unwrap();
                let id = self.next * 4;
                self.next += 1;
                let mut bytes = page(&entries, width as u8, kind, id, at as usize);
                bytes[4061] = level;
                let sum = crc(&bytes[..4072]);
                p32(&mut bytes, 4076, sum);
                self.file.write_all(&bytes).unwrap();
                parents.write_all(&key.to_le_bytes()).unwrap();
                parents.write_all(&id.to_le_bytes()).unwrap();
                parents.write_all(&at.to_le_bytes()).unwrap();
                last = (id, at);
                left -= n as u64;
            }
            drop(reader);
            drop(parents);
            std::fs::remove_file(&input).unwrap();
            if count <= (4056 / width) as u64 {
                std::fs::remove_file(parent).unwrap();
                return last;
            }
            input = parent;
            width = 24;
            level += 1;
        }
    }
    fn finish(mut self, nodes: BTreeMap<u32, (u32, u64, u64)>) {
        let nbt_path = self.directory.join("nbt");
        let mut nbt = File::create(&nbt_path).unwrap();
        for (nid, (parent, data, sub)) in nodes {
            let mut b = [0; 32];
            p64(&mut b, 0, u64::from(nid));
            p64(&mut b, 8, data);
            p64(&mut b, 16, sub);
            p32(&mut b, 24, parent);
            p32(&mut b, 28, 2);
            nbt.write_all(&b).unwrap();
        }
        drop(nbt);
        self.bbt.flush().unwrap();
        let nbt = self.tree(&nbt_path, 32, 0x81);
        // Release the BBT handle before the tree retires its input on Windows.
        let bbt_path = self.directory.join("bbt");
        let placeholder = File::create(self.directory.join("placeholder")).unwrap();
        drop(std::mem::replace(&mut self.bbt, placeholder));
        let bbt = self.tree(&bbt_path, 24, 0x80);
        let mut h = vec![0; 564];
        h[..4].copy_from_slice(b"!BDN");
        h[8..10].copy_from_slice(b"SO");
        p16(&mut h, 10, 36);
        p16(&mut h, 12, 19);
        h[14] = 1;
        h[15] = 1;
        p64(&mut h, 184, self.file.metadata().unwrap().len());
        p64(&mut h, 216, nbt.0);
        p64(&mut h, 224, nbt.1);
        p64(&mut h, 232, bbt.0);
        p64(&mut h, 240, bbt.1);
        h[248] = 1;
        h[512] = 0x80;
        let sum = crc(&h[8..479]);
        p32(&mut h, 4, sum);
        let sum = crc(&h[8..524]);
        p32(&mut h, 524, sum);
        self.file.seek(SeekFrom::Start(0)).unwrap();
        self.file.write_all(&h).unwrap();
        self.file.sync_all().unwrap();
        let directory = self.directory.clone();
        drop(self);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

pub fn payload_byte(item: u32, offset: usize) -> u8 {
    item.to_le_bytes()[offset % 4].wrapping_add((offset / 4 % 251) as u8)
}
/// Groups of at most 64 rows keep the independent TC encoder's small-heap limit.
/// NBT/BBT depth and total payload grow on disk, without a whole-store byte vector.
pub fn generate(path: &Path, count: u32, payload_len: usize) {
    let groups = count.div_ceil(64);
    let root = Folder {
        children: vec![Folder {
            children: vec![
                Folder {
                    role: Some(SystemFolder::Finder),
                    ..Default::default()
                },
                Folder {
                    role: Some(SystemFolder::IpmSubtree),
                    children: std::iter::once(Folder {
                        role: Some(SystemFolder::Wastebasket),
                        ..Default::default()
                    })
                    .chain((0..groups).map(|_| Folder::default()))
                    .collect(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut w = Writer {
        disk: Some(Disk::new(path)),
        ..Default::default()
    };
    let mut properties = vec![Property::binary(0x0ff9, vec![0x23; 16])];
    folder_roles(&root, &mut 0x120, &mut properties);
    w.node(0x21, 0, &properties);
    w.folder(&root, 0, &mut 0x120);
    for group in 0..groups {
        let folder = 0x1c2 + group * 32;
        let mut rows = Vec::new();
        for item in group * 64..count.min((group + 1) * 64) {
            let nid = 0x10004 + item * 32;
            let mut value = Item::new("IPM.Note", payload_len as u32);
            value.properties.push(Property::binary(
                0x6801,
                (0..payload_len).map(|at| payload_byte(item, at)).collect(),
            ));
            let (data, sub) = w.item(&value);
            w.nodes.insert(nid, (folder, data, sub));
            rows.push((nid, vec![]));
        }
        w.node(
            folder,
            0x182,
            &[
                Property::int(0x3602, rows.len() as u32),
                Property::int(0x3603, 0),
                Property::int(0x360a, 0),
            ],
        );
        let data = w.table(&rows);
        w.nodes.insert(folder & !31 | 0xe, (0, data, 0));
    }
    let nodes = std::mem::take(&mut w.nodes);
    w.disk.take().unwrap().finish(nodes);
}
