use crate::{ext4_h::*, file_tree::Directory, serialization::Buffer};
use std::io::{self, Cursor, Write};

mod ext4_h;
mod file_tree;
#[macro_use]
mod serialization;
mod util;

const BLOCK_SIZE: u64 = 4096;
const INODE_SIZE: u64 = 256;

pub trait BlockWriteDeviece {
    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> io::Result<()>;
}

impl<W: io::Write + io::Seek> BlockWriteDeviece for W {
    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> io::Result<()> {
        assert!(buf.len() <= BLOCK_SIZE as usize);
        self.seek(io::SeekFrom::Start(block_num * BLOCK_SIZE))?;
        self.write_all(buf)?;
        Ok(())
    }
}

#[derive(Default)]
struct UsageBitmap {
    data: Vec<u8>,
    next_free: u64,
}
impl UsageBitmap {
    fn mark_used(&mut self, block_num: u64) {
        let byte_index = (block_num / 8) as usize;
        let bit_index = (block_num % 8) as u8;
        if byte_index >= self.data.len() {
            self.data.resize(byte_index + 1, 0);
        }
        self.data[byte_index] |= 1 << bit_index;
    }
    fn get_for_block_group(&mut self, block_group: u64, len: u32) -> BitmapBlock {
        let start = (block_group * BLOCK_SIZE) as usize;
        let end = ((block_group + 1) * BLOCK_SIZE) as usize;
        if self.data.len() < end {
            self.data.resize(end, 0);
        }
        BitmapBlock::from_bytes(&self.data[start..end], len)
    }
    fn allocate(&mut self, n: u64) -> Allocation {
        let start = self.next_free;
        for i in 0..n {
            self.mark_used(self.next_free + i);
        }
        self.next_free += n;
        Allocation {
            start,
            end: self.next_free,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Allocation {
    pub start: u64,
    pub end: u64,
}
impl Allocation {
    pub fn from_start_len(start: u64, len: u64) -> Self {
        Allocation {
            start,
            end: start + len,
        }
    }
    pub fn as_single(self) -> u64 {
        assert!(self.end == self.start + 1);
        self.start
    }
}

pub struct Ext4ImageWriter<W: BlockWriteDeviece> {
    writer: W,
    uuid: [u8; 16],
    max_size: u64,

    directories: Directory,
    inodes: Vec<Ext4Inode>,
    used_blocks: UsageBitmap,
    used_inodes: UsageBitmap,
}
impl<W: BlockWriteDeviece> Ext4ImageWriter<W> {
    /// Create a new `Ext4ImageWriter` that writes to the given block device.
    /// The `max_size` parameter specifies the maximum size of the image in bytes (potentially after resizing).
    /// This is used to determine the space reserved for block group descriptors.
    pub fn new(writer: W, max_size: u64) -> Self {
        let mut this = Self {
            writer,
            uuid: [
                0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC,
                0xDE, 0xF0,
            ],
            max_size,

            directories: Default::default(),
            inodes: Default::default(),
            used_blocks: UsageBitmap::default(),
            used_inodes: UsageBitmap::default(),
        };
        this.used_blocks.allocate(1); // superblock
        this.used_blocks.allocate(this.bgdt_blocks());

        this.alloc_inode(); // inode 1 is the bad blocks inode
        this.alloc_inode(); // inode 2 is the root directory (we will populate it later)
        this.alloc_inode(); // inode 3 is the user quota inode (we won't use it)
        this.alloc_inode(); // inode 4 is the group quota inode (we won't use it)
        this.alloc_inode(); // inode 5 is the boot loader inode (we won't use it)
        this.alloc_inode(); // inode 6 is the undelete inode (we won't use it)
        this.alloc_inode(); // inode 7 is the resize inode
        this.alloc_inode(); // inode 8 is the journal inode (we won't use it)
        this.alloc_inode(); // inode 9 is the "exclude" inode (we won't use it)
        this.alloc_inode(); // inode 10 is for some obscure non-upstream feature (we won't use it)
        this.alloc_inode(); // inode 11 is the "lost+found" directory (we will populate it later)

        this.directories.mkdir("lost+found").unwrap();

        this
    }

    /// Write a file to the filesystem at the given path with the given mode.
    /// The path must use '/' as the separator.
    pub fn write_file(&mut self, contents: &[u8], path: &str, mode: u16) -> io::Result<()> {
        let inode_num = self.alloc_inode();
        let mut inode = self.create_inode_with_contents(contents, FileType::RegularFile)?;
        inode.set_mode(mode);
        self.inodes[(inode_num - 1) as usize] = inode;
        self.directories.create_file(path, inode_num)?;
        Ok(())
    }

    /// Create a directory at the given path. All parent directories must already exist.
    /// The path must use '/' as the separator.
    pub fn mkdir(&mut self, path: &str) -> io::Result<()> {
        self.directories.mkdir(path)?;
        Ok(())
    }

    /// Create a directory at the given path, creating all parent directories as needed.
    /// The path must use '/' as the separator.
    pub fn mkdir_p(&mut self, path: &str) -> io::Result<()> {
        self.directories.mkdir_p(path)?;
        Ok(())
    }

    /// Write all metadata to the underlying block device and finish writhing the filesystem
    pub fn finalize(mut self) -> io::Result<()> {
        let directories = std::mem::take(&mut self.directories);
        self.write_hierarchy_to_inodes(&directories, 2, 2)?;

        let num_inodes = self.inodes.len() as u64;
        let blocks_needed_for_inodes = (num_inodes * INODE_SIZE).div_ceil(BLOCK_SIZE);
        let blocks_estimate = self.used_blocks.next_free + blocks_needed_for_inodes + 1 /* resize inode indirect block */ ;
        let block_groups_estimate = blocks_estimate.div_ceil(BLOCK_SIZE * 8);
        let blocks_estimate = blocks_estimate + block_groups_estimate * 2; // for the block and inode bitmaps;
        let block_groups_estimate = blocks_estimate.div_ceil(BLOCK_SIZE * 8);
        let inodes_per_group = ((num_inodes / block_groups_estimate)
            .div_ceil(BLOCK_SIZE / INODE_SIZE)
            * (BLOCK_SIZE / INODE_SIZE)) as usize;
        assert!(
            block_groups_estimate == self.inodes.len().div_ceil(inodes_per_group as usize) as u64
        );

        self.inodes[6 /*inode 7*/] = self.create_resize_inode(block_groups_estimate)?;

        // write inodes and build block group descriptors for each block group.
        let mut total_free_inodes = 0;
        let mut total_free_blocks = 0;
        let mut bgdt_buf = Cursor::new(Vec::new());
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8) as u32;
        let mut inodes = std::mem::take(&mut self.inodes);
        for (block_group, inodes) in inodes.chunks_mut(inodes_per_group).enumerate() {
            if block_group >= max_bgdt_table_len as usize {
                panic!("too many block groups, try increasing the max_size parameter");
            }
            let mut inode_buf = Cursor::new(vec![0u8; inodes_per_group * INODE_SIZE as usize]);
            let mut directories = 0;
            for (i, inode) in inodes.iter_mut().enumerate() {
                let inode_num = (block_group * inodes_per_group + i + 1) as u32;
                inode.update_checksum(&self.uuid, inode_num);
                inode_buf.write_all(&inode.as_bytes())?;
                if inode.is_directory() {
                    directories += 1;
                }
            }

            // write out the inode table for this block group
            let block_bitmap_len = if block_group == block_groups_estimate as usize - 1 {
                (blocks_estimate % (BLOCK_SIZE * 8)) as u32
            } else {
                (BLOCK_SIZE * 8) as u32
            };
            // we need to allocate everything first to make sure that the block bitmaps are represented in themselves
            let block_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_table_alloc = self
                .used_blocks
                .allocate((inodes_per_group as u64 * INODE_SIZE).div_ceil(BLOCK_SIZE));
            let block_bitmap = self
                .used_blocks
                .get_for_block_group(block_group as u64, block_bitmap_len);
            self.write_blocks(block_bitmap_alloc, &block_bitmap.as_bytes())?;
            let inode_bitmap = self
                .used_inodes
                .get_for_block_group(block_group as u64, inodes_per_group as u32);
            self.write_blocks(inode_bitmap_alloc, &inode_bitmap.as_bytes())?;
            self.write_blocks(inode_table_alloc, &inode_buf.into_inner())?;
            let mut block_group_descriptor = Ext4BlockGroupDescriptor::default();
            block_group_descriptor.set_block_bitmap(block_bitmap_alloc.as_single());
            block_group_descriptor.set_free_blocks_count(block_bitmap.free_count());
            total_free_blocks += block_bitmap.free_count() as u64;
            block_group_descriptor.set_inode_bitmap(inode_bitmap_alloc.as_single());
            block_group_descriptor.set_free_inodes_count(inode_bitmap.free_count());
            total_free_inodes += inode_bitmap.free_count();
            block_group_descriptor.set_inode_table(inode_table_alloc.start);
            block_group_descriptor.set_used_dirs_count(directories);
            block_group_descriptor.update_checksums(
                &self.uuid,
                block_group as u32,
                &block_bitmap,
                &inode_bitmap,
            );
            bgdt_buf.write_all(&block_group_descriptor.as_bytes())?;
        }
        self.write_blocks(
            Allocation::from_start_len(1, self.bgdt_blocks()),
            &bgdt_buf.into_inner(),
        )?;

        assert_eq!(self.used_blocks.next_free, blocks_estimate);

        // finally write the superblock
        let mut superblock = ext4_h::Ext4SuperBlock::new(self.uuid, inodes_per_group as u32);
        let used_bgdt_blocks =
            (block_groups_estimate as u64 * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);
        superblock.set_reserved_gdt_blocks((self.bgdt_blocks() - used_bgdt_blocks) as u16);
        superblock.set_free_inodes_count(total_free_inodes);
        superblock.set_free_blocks_count(total_free_blocks);
        superblock.update_blocks_count(blocks_estimate);
        superblock.update_checksum();
        let mut first_block = [0u8; BLOCK_SIZE as usize];
        first_block[1024..1024 + 1024].copy_from_slice(&superblock.as_bytes());
        self.writer.write_block(0, &first_block)?;

        Ok(())
    }

    fn create_resize_inode(&mut self, block_groups: u64) -> io::Result<Ext4Inode> {
        // this is actually not correct since when we call this function it might still happen that we modify these values
        let used_bgdt_blocks =
            (block_groups as u64 * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);

        let bgdt_block_list = (1 + used_bgdt_blocks)..(self.bgdt_blocks() + 1);
        let mut indirect_buffer = vec![];
        indirect_buffer.extend_from_slice(&(0u32).to_le_bytes());
        for block in bgdt_block_list {
            self.used_blocks.mark_used(block);
            indirect_buffer.extend_from_slice(&(block as u32).to_le_bytes());
        }
        assert!(indirect_buffer.len() <= BLOCK_SIZE as usize);
        let block_indirect = self.write_blocks_alloc(&indirect_buffer)?;
        let descr = LegacyBlockDescriptor::new(block_indirect.as_single() as u32);
        let mut inode = Ext4Inode::default();

        descr.write_buffer(inode.block_mut());
        inode.update_size((self.bgdt_blocks() - used_bgdt_blocks + 1) * BLOCK_SIZE);
        inode.set_file_type(FileType::RegularFile);
        inode.set_links_count(1);
        inode.set_size(LegacyBlockDescriptor::maximum_addressable_size());
        Ok(inode)
    }

    fn bgdt_blocks(&self) -> u64 {
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8);
        (max_bgdt_table_len * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE)
    }

    fn write_hierarchy_to_inodes(
        &mut self,
        directory: &Directory,
        inode_num: u64,
        parent_inode_num: u64,
    ) -> io::Result<()> {
        let base_entries = vec![
            Ok(Ext4DirEntry::new(
                inode_num as u32,
                FileType::Directory,
                ".",
            )),
            Ok(Ext4DirEntry::new(
                parent_inode_num as u32,
                FileType::Directory,
                "..",
            )),
        ];
        let entries = base_entries
            .into_iter()
            .chain(directory.entries().iter().map(|(name, entry)| {
                Ok(match entry {
                    file_tree::DirectoryEntry::Directory(directory) => {
                        let entry_inode_num = if inode_num == 2 && name == "lost+found" {
                            11
                        } else {
                            self.alloc_inode()
                        };
                        self.write_hierarchy_to_inodes(directory, entry_inode_num, inode_num)?;
                        Ext4DirEntry::new(entry_inode_num as u32, FileType::Directory, name)
                    }
                    file_tree::DirectoryEntry::File(inode) => {
                        Ext4DirEntry::new(*inode as u32, FileType::RegularFile, name)
                    }
                })
            }))
            .collect::<io::Result<Vec<_>>>()?;
        self.inodes[inode_num as usize - 1] = self.create_directory_inode(inode_num, &entries)?;
        Ok(())
    }

    fn create_directory_inode(
        &mut self,
        inode_num: u64,
        entries: &[Ext4DirEntry],
    ) -> io::Result<Ext4Inode> {
        let mut dir_blocks = vec![LinearDirectoryBlock::default()];
        for entry in entries {
            if !dir_blocks.last().unwrap().fits(entry) {
                dir_blocks.push(LinearDirectoryBlock::default());
            }
            dir_blocks.last_mut().unwrap().add_entry(entry.clone());
        }
        let mut dir_buffer = vec![0u8; dir_blocks.len() * BLOCK_SIZE as usize];
        for (i, block) in dir_blocks.iter().enumerate() {
            let mut dir_block = block.clone();
            dir_block.update_checksum(&self.uuid, inode_num as u32, 0);
            dir_block.write_buffer(
                &mut dir_buffer[i * BLOCK_SIZE as usize..(i + 1) * BLOCK_SIZE as usize],
            );
        }
        let mut inode = self.create_inode_with_contents(&dir_buffer, FileType::Directory)?;
        let subdirectories = entries.iter().filter(|e| e.is_directory()).count();
        inode.set_links_count(2 + (subdirectories as u16 - 2)); // 1 for the parent, one for '.' and 1 for each subdirectory
        inode.set_mode(0o755);
        inode.update_size((dir_buffer.len() as u64).div_ceil(BLOCK_SIZE) * BLOCK_SIZE);
        Ok(inode)
    }

    fn create_inode_with_contents(
        &mut self,
        contents: &[u8],
        ty: FileType,
    ) -> io::Result<Ext4Inode> {
        let start_block = self.write_blocks_alloc(contents)?;
        let inode = Ext4Inode::new(contents.len() as u64, start_block, ty);
        Ok(inode)
    }

    fn alloc_inode(&mut self) -> u64 {
        let n = self.inodes.len() as u64;
        self.inodes.push(Ext4Inode::default());
        self.used_inodes.mark_used(n);
        n + 1
    }

    fn write_blocks(&mut self, allocation: Allocation, data: &[u8]) -> io::Result<()> {
        let mut offset = 0;
        let mut block_num = allocation.start;
        while offset < data.len() {
            let end = (offset + BLOCK_SIZE as usize).min(data.len());
            let mut block = [0u8; BLOCK_SIZE as usize];
            block[..end - offset].copy_from_slice(&data[offset..end]);
            self.writer.write_block(block_num, &block)?;
            offset += BLOCK_SIZE as usize;
            block_num += 1;
        }
        assert!(allocation.end >= block_num);
        Ok(())
    }

    fn write_blocks_alloc(&mut self, data: &[u8]) -> io::Result<Allocation> {
        let num_blocks = (data.len() as u64).div_ceil(BLOCK_SIZE);
        let allocation = self.used_blocks.allocate(num_blocks);
        self.write_blocks(allocation, data)?;
        Ok(allocation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ext4_image_writer_minimal() {
        let _ = std::fs::remove_file("target/minimal.img");
        let file = std::fs::File::create("target/minimal.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        writer.mkdir("/test-dir").unwrap();
        writer
            .write_file("hello, world".as_bytes(), "test-dir/hello.txt", 0o755)
            .unwrap();
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/smoke.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }

    #[test]
    fn test_ext4_image_writer_many_files() {
        let _ = std::fs::remove_file("target/many_files.img");
        let file = std::fs::File::create("target/many_files.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        for i in 0..5000 {
            writer
                .write_file(
                    format!("hello, world {i}").as_bytes(),
                    &format!("file-{i}.txt"),
                    0o755,
                )
                .unwrap();
        }
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/smoke.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }
}
