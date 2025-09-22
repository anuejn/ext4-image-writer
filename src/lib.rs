use crate::{ext4_h::*, serialization::Buffer};
use std::{
    collections::VecDeque,
    io::{self, Cursor, Write},
};

#[allow(dead_code)]
mod ext4_h;
#[macro_use]
mod serialization;

const BLOCK_SIZE: u64 = 4096;
const BLOCK_GROUP_SIZE: u64 = BLOCK_SIZE * BLOCK_SIZE * 8;
const INODE_SIZE: u64 = 256;

pub trait BlockWriteDeviece {
    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> io::Result<()>;
}

impl<W: io::Write + io::Seek> BlockWriteDeviece for W {
    fn write_block(&mut self, block_num: u64, buf: &[u8]) -> io::Result<()> {
        assert!(buf.len() <= BLOCK_SIZE as usize);
        self.seek(io::SeekFrom::Start(block_num * BLOCK_SIZE as u64))?;
        self.write_all(buf)?;
        Ok(())
    }
}

pub struct Ext4ImageWriter<W: BlockWriteDeviece> {
    writer: W,
    inodes: VecDeque<Ext4Inode>,
    max_size: u64,
    next_free_block: u64,
}
impl<W: BlockWriteDeviece> Ext4ImageWriter<W> {
    /// Create a new `Ext4ImageWriter` that writes to the given block device.
    /// The `max_size` parameter specifies the maximum size of the image in bytes (potentially after resizing).
    /// This is used to determine the space reserved for block group descriptors.
    pub fn new(writer: W, max_size: u64) -> Self {
        let bgdt_table_len = max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8);
        let bgdt_table_blocks = bgdt_table_len.div_ceil(64 * BLOCK_SIZE); // one bgd is 64 bytes

        let mut inodes = VecDeque::new();
        inodes.push_back(Ext4Inode::default()); // inode 1 is the bad blocks inode
        inodes.push_back(Ext4Inode::default()); // inode 2 is the root directory (we will populate it later)
        inodes.push_back(Ext4Inode::default()); // inode 3 is the user quota inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 4 is the group quota inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 5 is the boot loader inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 6 is the undelete inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 7 is the resize inode
        inodes.push_back(Ext4Inode::default()); // inode 8 is the journal inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 9 is the "exclude" inode (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 10 is for some obscure non-upstream feature (we won't use it)
        inodes.push_back(Ext4Inode::default()); // inode 11 is the "lost+found" directory (we will populate it later)

        Self {
            writer,
            inodes,
            max_size,
            next_free_block: 1 + bgdt_table_blocks + 1, // superblock + bgdt
        }
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> io::Result<()> {
        let start_block = self.write_blocks_alloc(data)?;
        let inode = Ext4Inode::new(data.len() as u64, start_block);
        self.inodes.push_back(inode);
        Ok(())
    }

    pub fn finalize(mut self) -> io::Result<()> {
        let mut superblock = ext4_h::Ext4SuperBlock::new();
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8);
        let bgdt_blocks =
            (max_bgdt_table_len * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);
        superblock.set_reserved_gdt_blocks(bgdt_blocks as u16);

        // set the resize inode
        let bgdt_block_list = (1..(bgdt_blocks as u32 + 1)).collect::<Vec<_>>();
        let resize_inode_blocks =
            self.create_double_indirect_legacy_block_descriptor(&bgdt_block_list)?;
        let resize_inode = &mut self.inodes[6 /*inode 7*/];
        resize_inode_blocks.write_buffer(resize_inode.block_mut());
        resize_inode.set_file_type(FileType::RegularFile);
        resize_inode.set_links_count(1);

        // setup directory hierarchy
        // TODO:

        // setup the root directory inode

        // we now analyze what we have written and build block group descriptors for each block group.
        let mut bgdt_buf = Cursor::new(Vec::new());
        let mut last_block_group = 0;
        let mut inode_num: u32 = 1;
        while !self.inodes.is_empty() {
            // for each block group
            let block_bitmap = BitmapBlock::new(4096);
            let mut inode_bitmap = BitmapBlock::new(superblock.inodes_per_group());

            let mut inode_buf = Cursor::new(vec![
                0u8;
                superblock.inodes_per_group() as usize
                    * INODE_SIZE as usize
            ]);
            while let Some(mut inode) = self.inodes.pop_front() {
                let block_group = (inode_num - 1) / superblock.inodes_per_group();
                let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8) as u32;
                if last_block_group >= max_bgdt_table_len {
                    panic!("too many block groups");
                }
                if block_group != last_block_group {
                    self.inodes.push_front(inode); // put it back for the next block group
                    break; // we are done with this block group
                }
                inode.update_checksum(superblock.uuid(), inode_num);
                println!("Inode {}: {:#?}", inode_num, inode);
                inode_buf.write_all(&inode.as_buffer())?;
                inode_bitmap.set_bit((inode_num - 1) % superblock.inodes_per_group());

                inode_num += 1;
            }

            // write out the inode table for this block group
            let block_bitmap_block = self.write_blocks_alloc(&block_bitmap.as_buffer())?;
            let inode_bitmap_block = self.write_blocks_alloc(&inode_bitmap.as_buffer())?;
            let inode_block = self.write_blocks_alloc(&inode_buf.into_inner())?;
            let mut block_group_descriptor = Ext4BlockGroupDescriptor::default();
            block_group_descriptor.set_block_bitmap(block_bitmap_block);
            block_group_descriptor.set_inode_bitmap(inode_bitmap_block);
            block_group_descriptor.set_inode_table(inode_block);
            block_group_descriptor.update_checksums(
                superblock.uuid(),
                last_block_group,
                &block_bitmap,
                &inode_bitmap,
            );
            bgdt_buf.write_all(&block_group_descriptor.as_buffer())?;
            last_block_group += 1;
        }
        self.writer.write_block(1, &bgdt_buf.into_inner())?;

        // finally write the superblock
        superblock.update_blocks_count(self.next_free_block, inode_num - 1);
        superblock.update_checksum();
        let mut first_block = [0u8; BLOCK_SIZE as usize];
        first_block[1024..1024 + 1024].copy_from_slice(&superblock.as_buffer());
        self.writer.write_block(0, &first_block)?;

        Ok(())
    }

    pub fn create_inode_with_contents(&mut self, contents: &[u8]) -> io::Result<Ext4Inode> {
        let start_block = self.write_blocks_alloc(contents)?;
        let inode = Ext4Inode::new(contents.len() as u64, start_block);
        Ok(inode)
    }

    pub fn create_double_indirect_legacy_block_descriptor(
        &mut self,
        blocks: &[u32],
    ) -> io::Result<LegacyBlockDescriptor> {
        let mut double_indirect_buffer = vec![];
        for block in blocks {
            double_indirect_buffer.extend_from_slice(&block.to_le_bytes());
        }
        let blocks_count = double_indirect_buffer.len().div_ceil(BLOCK_SIZE as usize);
        let start_block_double_indirect = self.write_blocks_alloc(&double_indirect_buffer)?;
        let mut indirect_buffer = vec![];
        for block in 0..blocks_count {
            indirect_buffer.extend_from_slice(
                &(start_block_double_indirect as u32 + block as u32).to_le_bytes(),
            );
        }
        assert!(indirect_buffer.len() <= BLOCK_SIZE as usize);
        let block_indirect = self.write_blocks_alloc(&indirect_buffer)?;
        let descr = LegacyBlockDescriptor::new(block_indirect as u32);
        Ok(descr)
    }

    fn write_contents_and_inode(&mut self, data: &[u8]) -> io::Result<u64> {
        let start_block = self.write_blocks_alloc(data)?;
        let inode = Ext4Inode::new(data.len() as u64, start_block);
        self.inodes.push_back(inode);
        Ok(start_block)
    }

    fn alloc_blocks(&mut self, n: u64) -> u64 {
        let start = self.next_free_block;
        self.next_free_block += n;
        start
    }

    fn write_blocks(&mut self, start_block: u64, data: &[u8]) -> io::Result<()> {
        let mut offset = 0;
        let mut block_num = start_block;
        while offset < data.len() {
            let end = (offset + BLOCK_SIZE as usize).min(data.len());
            let mut block = [0u8; BLOCK_SIZE as usize];
            block[..end - offset].copy_from_slice(&data[offset..end]);
            self.writer.write_block(block_num as u64, &block)?;
            offset += BLOCK_SIZE as usize;
            block_num += 1;
        }
        Ok(())
    }

    fn write_blocks_alloc(&mut self, data: &[u8]) -> io::Result<u64> {
        let num_blocks = (data.len() as u64).div_ceil(BLOCK_SIZE);
        let start_block = self.alloc_blocks(num_blocks);
        self.write_blocks(start_block, data)?;
        Ok(start_block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ext4_image_writer_smoke() {
        let file = std::fs::File::create("target/smoke.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 10 * 1024 * 1024);
        writer.write_file("hello.txt", b"Hello, World!").unwrap();
        writer.finalize().unwrap();
    }
}
