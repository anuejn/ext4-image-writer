#![doc = include_str!("../README.md")]

use crate::{ext4_h::*, file_tree::Directory, serialization::Buffer};
use std::io::{self, Cursor, Write};

mod ext4_h;
mod file_tree;
#[macro_use]
mod serialization;
mod util;

const BLOCK_SIZE: u64 = 4096;

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
        let mut inode =
            self.create_inode_with_contents(inode_num as u32, contents, FileType::RegularFile)?;
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
        let blocks_needed_for_inodes = (num_inodes * Ext4Inode::SIZE).div_ceil(BLOCK_SIZE);
        let num_blocks = self.used_blocks.next_free + blocks_needed_for_inodes + 1 /* resize inode indirect block */ ;
        let num_block_groups = num_blocks.div_ceil(BLOCK_SIZE * 8);
        let num_blocks = num_blocks + num_block_groups * 2; // for the block and inode bitmaps;
        let num_block_groups = num_blocks.div_ceil(BLOCK_SIZE * 8);
        let inodes_per_group = ((num_inodes / num_block_groups)
            .div_ceil(BLOCK_SIZE / Ext4Inode::SIZE)
            * (BLOCK_SIZE / Ext4Inode::SIZE)) as usize;
        assert!(num_block_groups >= self.inodes.len().div_ceil(inodes_per_group) as u64);
        let num_blocks = self.used_blocks.next_free
            + (inodes_per_group as u64 * Ext4Inode::SIZE).div_ceil(BLOCK_SIZE) * num_block_groups
            + num_block_groups * 2 // for the block and inode bitmaps
            + 1; // resize inode indirect block

        self.inodes[6 /*inode 7*/] = self.create_resize_inode(num_block_groups)?;

        // write inodes and build block group descriptors for each block group.
        let mut total_free_inodes = 0;
        let mut total_free_blocks = 0;
        let mut bgdt_buf = Cursor::new(Vec::new());
        let max_bgdt_table_len = self.max_size.div_ceil(BLOCK_SIZE * BLOCK_SIZE * 8) as u32;
        let mut inodes = std::mem::take(&mut self.inodes);
        inodes.resize(
            num_block_groups as usize * inodes_per_group,
            Ext4Inode::default(),
        );
        for (block_group, inodes) in inodes.chunks_mut(inodes_per_group).enumerate() {
            if block_group >= max_bgdt_table_len as usize {
                panic!("too many block groups, try increasing the max_size parameter");
            }
            let mut inode_buf = Cursor::new(vec![0u8; inodes_per_group * Ext4Inode::SIZE as usize]);
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
            let block_bitmap_len = if block_group == num_block_groups as usize - 1 {
                (num_blocks % (BLOCK_SIZE * 8)) as u32
            } else {
                (BLOCK_SIZE * 8) as u32
            };
            // we need to allocate everything first to make sure that the block bitmaps are represented in themselves
            let block_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_bitmap_alloc = self.used_blocks.allocate(1);
            let inode_table_alloc = self
                .used_blocks
                .allocate((inodes_per_group as u64 * Ext4Inode::SIZE).div_ceil(BLOCK_SIZE));
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

        assert_eq!(self.used_blocks.next_free, num_blocks);

        // finally write the superblock
        let mut superblock = ext4_h::Ext4SuperBlock::new(self.uuid, inodes_per_group as u32);
        let used_bgdt_blocks =
            (num_block_groups * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);
        superblock
            .set_reserved_gdt_blocks((self.bgdt_blocks() - used_bgdt_blocks).try_into().unwrap());
        superblock.set_free_inodes_count(total_free_inodes);
        superblock.set_free_blocks_count(total_free_blocks);
        superblock.update_blocks_count(num_blocks);
        superblock.update_checksum();
        let mut first_block = [0u8; BLOCK_SIZE as usize];
        first_block[1024..1024 + 1024].copy_from_slice(&superblock.as_bytes());
        self.writer.write_block(0, &first_block)?;

        Ok(())
    }

    fn create_resize_inode(&mut self, block_groups: u64) -> io::Result<Ext4Inode> {
        // this is actually not correct since when we call this function it might still happen that we modify these values
        let used_bgdt_blocks = (block_groups * Ext4BlockGroupDescriptor::SIZE).div_ceil(BLOCK_SIZE);

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

        self.inodes[inode_num as usize - 1] = self.create_directory_inode(
            inode_num,
            &entries,
            inode_num != 11, /* lost+found cant be inline */
        )?;
        Ok(())
    }

    fn create_directory_inode(
        &mut self,
        inode_num: u64,
        entries: &[Ext4DirEntry],
        allow_inline: bool,
    ) -> io::Result<Ext4Inode> {
        let mut inode = if let Some(inode) = self.create_directory_inode_inline(entries)
            && allow_inline
        {
            inode
        } else {
            self.create_directory_inode_with_blocks(inode_num, entries)?
        };
        let subdirectories = entries.iter().filter(|e| e.is_directory()).count();
        inode.set_links_count(2 + (<u16>::try_from(subdirectories).unwrap() - 2)); // 1 for the parent, one for '.' and 1 for each subdirectory
        inode.set_mode(0o755);
        Ok(inode)
    }

    fn create_directory_inode_inline(&mut self, entries: &[Ext4DirEntry]) -> Option<Ext4Inode> {
        let mut block_entries =
            InlineLinearDirectoryBlock::new(Ext4Inode::MAX_INLINE_SIZE_BLOCK - 4);
        let mut xattr_entries = InlineLinearDirectoryBlock::new(Ext4Inode::MAX_INLINE_SIZE_XATTR);
        for entry in entries[2..].iter() {
            if block_entries.fits(entry) {
                block_entries.add_entry(entry.clone());
            } else if xattr_entries.fits(entry) {
                xattr_entries.add_entry(entry.clone());
            } else {
                return None; // cant fit entries inline
            }
        }

        let parent_inode = entries[1].inode();
        let mut block_data = [0u8; Ext4Inode::MAX_INLINE_SIZE_BLOCK];
        block_data[0..4].copy_from_slice(&parent_inode.to_le_bytes());
        block_data[4..].copy_from_slice(&block_entries.as_bytes());

        Some(Ext4Inode::with_inline_data(
            &block_data,
            &xattr_entries.as_bytes(),
            FileType::Directory,
        ))
    }

    fn create_directory_inode_with_blocks(
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
        self.create_inode_with_contents(inode_num as u32, &dir_buffer, FileType::Directory)
    }

    fn create_inode_with_contents(
        &mut self,
        inode_num: u32,
        contents: &[u8],
        ty: FileType,
    ) -> io::Result<Ext4Inode> {
        if contents.len() <= Ext4Inode::MAX_INLINE_SIZE {
            let block_data = &contents[..Ext4Inode::MAX_INLINE_SIZE_BLOCK.min(contents.len())];
            let xattr_data = if contents.len() > Ext4Inode::MAX_INLINE_SIZE_BLOCK {
                &contents[Ext4Inode::MAX_INLINE_SIZE_BLOCK..]
            } else {
                &[]
            };
            Ok(Ext4Inode::with_inline_data(block_data, xattr_data, ty))
        } else {
            let allocation = self.write_blocks_alloc(contents)?;
            let inode =
                self.create_inode_with_extents(inode_num, contents.len() as u64, allocation, ty)?;
            Ok(inode)
        }
    }

    fn create_inode_with_extents(
        &mut self,
        inode_num: u32,
        size: u64,
        allocation: Allocation,
        ty: FileType,
    ) -> io::Result<Ext4Inode> {
        let blocks = allocation.end - allocation.start;
        if blocks <= Ext4InlineExtents::MAX_INLINE_BLOCKS {
            // we can fit the extents inline into the inode
            Ok(Ext4Inode::new(size, Ext4InlineExtents::new(allocation), ty))
        } else {
            // we need to allocate a separate block for the extents
            let indirect_block =
                Ext4IndirectExtents::create_block(allocation, inode_num, &self.uuid);
            let indirect_block_allocation = self.write_blocks_alloc(&indirect_block)?;
            let extents = Ext4IndirectExtents::new(indirect_block_allocation.start);
            let mut inode = Ext4Inode::new(size, extents, ty);
            inode.set_blocks(inode.blocks() + 8); // account for the indirect block
            Ok(inode)
        }
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
        let writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024);
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/minimal.img")
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
            .arg("target/many_files.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }

    #[test]
    fn test_ext4_image_writer_zero_size_file() {
        let _ = std::fs::remove_file("target/zero_size_file.img");
        let file = std::fs::File::create("target/zero_size_file.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        let zero_size_file = vec![];
        writer
            .write_file(&zero_size_file, "zero_size_file.bin", 0o644)
            .unwrap();
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/zero_size_file.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }

    #[test]
    fn test_ext4_image_writer_big_file() {
        let _ = std::fs::remove_file("target/big_file.img");
        let file = std::fs::File::create("target/big_file.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        let big_file = vec![0xABu8; 1024 * 1024 * 1024];
        writer.write_file(&big_file, "big-file.bin", 0o644).unwrap();
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/big_file.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }

    #[test]
    fn test_ext4_image_writer_inline_dirs() {
        let _ = std::fs::remove_file("target/inline_dirs.img");
        let file = std::fs::File::create("target/inline_dirs.img").unwrap();
        let mut writer = Ext4ImageWriter::new(file, 1024 * 1024 * 1024 * 128);
        writer.mkdir("dir").unwrap();
        writer.write_file(&[], "dir/longer_entry", 0o755).unwrap();
        writer.write_file(&[], "dir/short_entry", 0o755).unwrap();
        writer.write_file(&[], "dir/over_the_edge", 0o755).unwrap();
        writer.finalize().unwrap();
        let process = std::process::Command::new("e2fsck")
            .arg("-f")
            .arg("-n")
            .arg("target/inline_dirs.img")
            .output()
            .unwrap();
        assert!(process.status.success());
    }
}
