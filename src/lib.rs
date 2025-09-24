use crate::{ext4_h::*, serialization::Buffer};
use std::io::{self, Cursor, Write};

mod ext4_h;
#[macro_use]
mod serialization;
mod util;

const BLOCK_SIZE: u64 = 4096;
const BLOCK_GROUP_SIZE: u64 = BLOCK_SIZE * BLOCK_SIZE * 8;
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
        println!("Allocating {} blocks at {}", n, self.next_free);
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
pub struct Allocation {
    pub start: u64,
    pub end: u64,
}
impl Allocation {
    pub fn single(block: u64) -> Self {
        Allocation {
            start: block,
            end: block + 1,
        }
    }
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
    inodes_per_group: u32,

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
            inodes_per_group: 4096,
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

        this
    }

    pub fn finalize(mut self) -> io::Result<()> {
        // set the resize inode
        self.inodes[6 /*inode 7*/] = self.create_resize_inode()?;

        // setup the lost+found directory
        let lost_and_found_inode = self.create_directory(
            11,
            &[
                (11, FileType::Directory, "."),
                (2, FileType::Directory, ".."),
            ],
        )?;
        self.inodes[10 /*inode 11*/] = lost_and_found_inode;

        // root directory inode
        let root_dir_inode = self.create_directory(
            2,
            &[
                (2, FileType::Directory, "."),
                (2, FileType::Directory, ".."),
                (11, FileType::Directory, "lost+found"),
            ],
        )?;
        self.inodes[1 /*inode 2*/] = root_dir_inode;

        // write inodes and build block group descriptors for each block group.
        let mut total_free_inodes = 0;
        let mut total_free_blocks = 0;
        let mut bgdt_buf = Cursor::new(Vec::new());
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8) as u32;
        let mut inodes = std::mem::take(&mut self.inodes);
        for (block_group, inodes) in inodes
            .chunks_mut(self.inodes_per_group as usize)
            .enumerate()
        {
            if block_group >= max_bgdt_table_len as usize {
                panic!("too many block groups, try increasing the max_size parameter");
            }
            let mut inode_buf = Cursor::new(vec![
                0u8;
                self.inodes_per_group as usize
                    * INODE_SIZE as usize
            ]);
            let mut directories = 0;
            for (i, inode) in inodes.iter_mut().enumerate() {
                let inode_num = (block_group * self.inodes_per_group as usize + i + 1) as u32;
                inode.update_checksum(&self.uuid, inode_num);
                inode_buf.write_all(&inode.as_bytes())?;
                if inode.is_directory() {
                    directories += 1;
                }
            }

            // write out the inode table for this block group
            let block_bitmap_len = if block_group == self.block_groups() as usize - 1 {
                (self.blocks() % BLOCK_GROUP_SIZE
                    + (self.inodes_per_group as u64 * INODE_SIZE).div_ceil(BLOCK_SIZE)
                    + 1
                    + 1) as u32
            } else {
                BLOCK_GROUP_SIZE as u32
            };
            // we need to allocate everything first to make sure that the block bitmaps are represented in themselves
            let block_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_table_alloc = self
                .used_blocks
                .allocate((self.inodes_per_group as u64 * INODE_SIZE).div_ceil(BLOCK_SIZE));
            let block_bitmap = self
                .used_blocks
                .get_for_block_group(block_group as u64, block_bitmap_len);
            self.write_blocks(block_bitmap_alloc, &block_bitmap.as_bytes())?;
            let inode_bitmap = self
                .used_inodes
                .get_for_block_group(block_group as u64, self.inodes_per_group);
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

        // finally write the superblock
        let mut superblock = ext4_h::Ext4SuperBlock::new(self.uuid, self.inodes_per_group);
        superblock.set_reserved_gdt_blocks(self.bgdt_blocks() as u16);
        superblock.set_free_inodes_count(dbg!(total_free_inodes));
        superblock.set_free_blocks_count(total_free_blocks);
        superblock.update_blocks_count(self.used_blocks.next_free);
        superblock.update_checksum();
        let mut first_block = [0u8; BLOCK_SIZE as usize];
        first_block[1024..1024 + 1024].copy_from_slice(&superblock.as_bytes());
        self.writer.write_block(0, &first_block)?;

        Ok(())
    }

    pub fn create_directory(
        &mut self,
        inode_num: u64,
        entries: &[(u32, FileType, &str)],
    ) -> io::Result<Ext4Inode> {
        let mut dir_blocks = vec![LinearDirectoryBlock::default()];
        for (inode, ty, name) in entries {
            let entry = Ext4DirEntry::new(*inode, *ty, name);
            if !dir_blocks.last().unwrap().fits(&entry) {
                dir_blocks.push(LinearDirectoryBlock::default());
            }
            dir_blocks.last_mut().unwrap().add_entry(entry);
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
        inode.set_links_count(2 + (entries.len() as u16 - 2)); // 1 for the parent, one for '.' and 1 for each subdirectory
        inode.set_mode(0o755);
        inode.update_size((dir_buffer.len() as u64).div_ceil(BLOCK_SIZE) * BLOCK_SIZE);
        Ok(inode)
    }

    /// current size in blocks of the filesystem
    pub fn blocks(&self) -> u64 {
        self.used_blocks.next_free
    }

    /// current number of block groups
    pub fn block_groups(&self) -> u32 {
        (self.blocks().div_ceil(BLOCK_SIZE) as u32)
            .max(self.inodes.len() as u32 / self.inodes_per_group)
    }

    pub fn create_inode_with_contents(
        &mut self,
        contents: &[u8],
        ty: FileType,
    ) -> io::Result<Ext4Inode> {
        let start_block = self.write_blocks_alloc(contents)?;
        let inode = Ext4Inode::new(contents.len() as u64, start_block, ty);
        Ok(inode)
    }

    pub fn alloc_inode(&mut self) -> u64 {
        let n = self.inodes.len() as u64;
        self.inodes.push(Ext4Inode::default());
        self.used_inodes.mark_used(n);
        n + 1
    }

    pub fn create_resize_inode(&mut self) -> io::Result<Ext4Inode> {
        // this is actually not correct since when we call this function it might still happen that we modify these values
        let block_groups = self.block_groups();
        let used_bgdt_blocks =
            (block_groups as u64 * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);

        let bgdt_block_list = (1 + used_bgdt_blocks)..(1 + self.bgdt_blocks());
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
        inode.update_size(self.bgdt_blocks() * BLOCK_SIZE);
        inode.set_file_type(FileType::RegularFile);
        inode.set_links_count(1);
        inode.set_size(LegacyBlockDescriptor::maximum_addressable_size());
        Ok(inode)
    }

    fn bgdt_blocks(&self) -> u64 {
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8);
        (max_bgdt_table_len * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE)
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
    fn test_ext4_image_writer_smoke() {
        let _ = std::fs::remove_file("target/smoke.img");
        let file = std::fs::File::create("target/smoke.img").unwrap();
        let writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        writer.finalize().unwrap();
    }
}
