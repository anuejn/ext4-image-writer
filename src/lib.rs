use crate::ext4_h::*;
use binrw::{BinResult, BinWrite};
use std::io::{self, Cursor};

#[allow(dead_code)]
mod ext4_h;
mod serdes;

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
    inodes: Vec<Ext4Inode>,
    max_size: u64,
    next_free_block: u64,
}
impl<W: BlockWriteDeviece> Ext4ImageWriter<W> {
    /// Create a new `Ext4ImageWriter` that writes to the given writer.
    /// The `max_size` parameter specifies the maximum size of the image in bytes.
    /// This is used to determine the number of block groups and other parameters.
    pub fn new(writer: W, max_size: u64) -> Self {
        let bgdt_table_len = max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8);
        let bgdt_table_blocks = bgdt_table_len.div_ceil(64 * BLOCK_SIZE); // one bgd is 64 bytes

        Self {
            writer,
            inodes: vec![Ext4Inode::default(); 10], // the first 10 inodes are reserved
            max_size,
            next_free_block: 1 + bgdt_table_blocks + 1, // superblock + bgdt
        }
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> BinResult<()> {
        let start_block = self.write_blocks_alloc(data)?;
        let inode = Ext4Inode::new(data.len() as u64, start_block);
        self.inodes.push(inode);
        Ok(())
    }

    pub fn finalize(mut self) -> BinResult<()> {
        let mut first_block = Cursor::new([0u8; BLOCK_SIZE as usize]);
        let superblock = ext4_h::Ext4SuperBlock::new();
        superblock.write_le(&mut first_block)?;
        self.writer.write_block(1, &first_block.into_inner())?;

        // we now analyze what we have written and build block group descriptors for each block group.
        let mut bgdt_buf = Cursor::new(Vec::new());
        let mut last_block_group = 0;
        let mut inode_num = 0;
        while !self.inodes.is_empty() {
            // for each block group
            let block_bitmap = BitmapBlock::new(4096);
            let inode_bitmap = BitmapBlock::new(superblock.inodes_per_group());

            let mut inode_buf = Cursor::new(vec![0u8; superblock.inodes_per_group() as usize * INODE_SIZE as usize]);
            while let Some(inode) = self.inodes.pop() {
                inode_num += 1;
                let block_group = inode.block_group();
                let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8) as u32;
                if last_block_group >= max_bgdt_table_len {
                    panic!("too many block groups");
                }
                if block_group != last_block_group {
                    self.inodes.push(inode); // put it back for the next block group
                    break; // we are done with this block group
                }
                inode.with_checksum(superblock.uuid(), inode_num).write_le(&mut inode_buf)?;
            }

            // write out the inode table for this block group
            let block_bitmap_block = self.write_blocks_alloc(&binwrite_as_buf(&block_bitmap)?)?; // TODO: actually build a bitmap
            let inode_bitmap_block = self.write_blocks_alloc(&binwrite_as_buf(&inode_bitmap)?)?;
            let inode_block = self.write_blocks_alloc(&inode_buf.into_inner())?;
            let block_group_descriptor = Ext4BlockGroupDescriptor::default()
                .with_block_bitmap(block_bitmap_block)
                .with_inode_bitmap(inode_bitmap_block)
                .with_inode_table(inode_block);
            block_group_descriptor.with_checksums(superblock.uuid(), last_block_group, &block_bitmap, &inode_bitmap).write_le(&mut bgdt_buf)?;
            last_block_group += 1;
        }
        self.writer.write_block(1, &bgdt_buf.into_inner())?;

        // finally write the superblock
        let mut superblock_buf = Cursor::new([0u8; BLOCK_SIZE as usize]);
        superblock_buf.set_position(1024);
        let superblock = ext4_h::Ext4SuperBlock::new()
            .with_blocks_count(self.next_free_block);
        superblock.write_le(&mut superblock_buf)?;
        self.writer.write_block(0, &superblock_buf.into_inner())?;

        Ok(())
    }

    fn alloc_blocks(&mut self, n: u64) -> u64 {
        let start = self.next_free_block;
        self.next_free_block += n;
        start
    }

    fn write_blocks(&mut self, start_block: u64, data: &[u8]) -> BinResult<()> {
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

    fn write_blocks_alloc(&mut self, data: &[u8]) -> BinResult<u64> {
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
