use crate::serialization::{
    Buffer, StaticLenString, buffer_struct, hi_lo_field_u32, hi_lo_field_u48, hi_lo_field_u64,
    impl_buffer_for_array,
};
use crate::{Allocation, BLOCK_SIZE};
use std::fmt::Debug;

macro_rules! calculate_checksum {
    ($($item:expr),*) => {
        {
            let mut crc = 0;
            $(
                crc = crc32c::crc32c_append(crc, $item);
            )*
            0xffffffff - crc
        }
    };
}

buffer_struct! { Ext4SuperBlock {
    /*00*/ s_inodes_count: u32,         /* Inodes count */
    s_blocks_count_lo: u32,      /* Blocks count */
    s_r_blocks_count_lo: u32,    /* Reserved blocks count */
    s_free_blocks_count_lo: u32, /* Free blocks count */
    /*10*/ s_free_inodes_count: u32, /* Free inodes count */
    s_first_data_block: u32,  /* First Data Block */
    s_log_block_size: u32 = 2,    /* Block size */
    s_log_cluster_size: u32 = 2,  /* Allocation cluster size */
    /*20*/ s_blocks_per_group: u32,   /* # Blocks per group */
    s_clusters_per_group: u32, /* # Clusters per group */
    s_inodes_per_group: u32,   /* # Inodes per group */
    s_mtime: u32,              /* Mount time */
    /*30*/ s_wtime: u32,           /* Write time */
    s_mnt_count: u16,       /* Mount count */
    s_max_mnt_count: u16,   /* Maximal mount count */
    s_magic: u16 = 0xEF53,           /* Magic signature */
    s_state: u16,           /* File system state */
    s_errors: u16,          /* Behaviour when detecting errors */
    s_minor_rev_level: u16, /* minor revision level */
    /*40*/ s_lastcheck: u32,     /* time of last check */
    s_checkinterval: u32, /* max. time between checks */
    s_creator_os: u32 = 0,    /* OS */
    s_rev_level: u32,     /* Revision level */
    /*50*/ s_def_resuid: u16, /* Default uid for reserved blocks */
    s_def_resgid: u16, /* Default gid for reserved blocks */
    /*
     * These fields are for EXT4_DYNAMIC_REV superblocks only.
     *
     * Note: the difference between the compatible feature set and
     * the incompatible feature set is that if there is a bit set
     * in the incompatible feature set that the kernel doesn't
     * know about, it should refuse to mount the filesystem.
     *
     * e2fsck's requirements are strict: more, if it doesn't know
     * about a feature in either the compatible or incompatible
     * feature set, it must abort and not try to meddle with
     * things it doesn't understand...
     */
    s_first_ino: u32,      /* First non-reserved inode */
    s_inode_size: u16,     /* size of inode structure */
    s_block_group_nr: u16, /* block group # of this superblock */
    s_feature_compat: u32, /* compatible feature set */
    /*60*/ s_feature_incompat: u32,  /* incompatible feature set */
    s_feature_ro_compat: u32, /* readonly-compatible feature set */
    /*68*/ s_uuid: [u8; 16], /* 128-bit uuid for volume */
    /*78*/ s_volume_name: StaticLenString<16>, /* volume name */
    /*88*/ s_last_mounted: StaticLenString<64>, /* directory where last mounted */
    /*C8*/ s_algorithm_usage_bitmap: u32, /* For compression */
    /*
     * Performance hints.  Directory preallocation should only
     * happen if the EXT4_FEATURE_COMPAT_DIR_PREALLOC flag is on.
     */
    s_prealloc_blocks: u8,      /* Nr of blocks to try to preallocate*/
    s_prealloc_dir_blocks: u8,  /* Nr to preallocate for dirs */
    s_reserved_gdt_blocks: u16, /* Per group desc for online growth */
    /*
     * Journaling support valid if EXT4_FEATURE_COMPAT_HAS_JOURNAL set.
     */
    /*D0*/
    s_journal_uuid: [u8; 16], /* uuid of journal superblock */
    /*E0*/ s_journal_inum: u32,    /* inode number of journal file */
    s_journal_dev: u32,     /* device number of journal file */
    s_last_orphan: u32,     /* start of list of inodes to delete */
    s_hash_seed: [u32; 4],  /* HTREE hash seed */
    s_def_hash_version: u8, /* Default hash version to use */
    s_jnl_backup_type: u8,
    s_desc_size: u16 = 64, /* size of group descriptor */
    /*100*/ s_default_mount_opts: u32,
    s_first_meta_bg: u32,    /* First metablock block group */
    s_mkfs_time: u32,        /* When the filesystem was created */
    s_jnl_blocks: [u32; 17], /* Backup of the journal inode */
    /* 64bit support valid if EXT4_FEATURE_INCOMPAT_64BIT */
    /*150*/
    s_blocks_count_hi: u32,      /* Blocks count */
    s_r_blocks_count_hi: u32,    /* Reserved blocks count */
    s_free_blocks_count_hi: u32, /* Free blocks count */
    s_min_extra_isize: u16,      /* All inodes have at least # bytes */
    s_want_extra_isize: u16,     /* New inodes should reserve # bytes */
    s_flags: u32,                /* Miscellaneous flags */
    s_raid_stride: u16,          /* RAID stride */
    s_mmp_update_interval: u16,  /* # seconds to wait in MMP checking */
    s_mmp_block: u64,            /* Block for multi-mount protection */
    s_raid_stripe_width: u32,    /* blocks on all data disks (N*stride)*/
    s_log_groups_per_flex: u8,   /* FLEX_BG group size */
    s_checksum_type: u8 = 1,         /* metadata checksum algorithm used */
    s_encryption_level: u8,      /* versioning level for encryption */
    s_reserved_pad: u8,          /* Padding to next 32bits */
    s_kbytes_written: u64,       /* nr of lifetime kilobytes written */
    s_snapshot_inum: u32,        /* Inode number of active snapshot */
    s_snapshot_id: u32,          /* sequential ID of active snapshot */
    s_snapshot_r_blocks_count: u64, /* reserved blocks for active
                                 snapshot's future use */
    s_snapshot_list: u32,                    /* inode number of the head of the
                                             on-disk snapshot list */
    s_error_count: u32,                      /* number of fs errors */
    s_first_error_time: u32,                 /* first time an error happened */
    s_first_error_ino: u32,                  /* inode involved in first error */
    s_first_error_block: u64,                /* block involved of first error */
    s_first_error_func: StaticLenString<32>, /* function where the error happened */
    s_first_error_line: u32,                 /* line number where error happened */
    s_last_error_time: u32,                  /* most recent time of an error */
    s_last_error_ino: u32,                   /* inode involved in last error */
    s_last_error_line: u32,                  /* line number where error happened */
    s_last_error_block: u64,                 /* block involved of last error */
    s_last_error_func: StaticLenString<32>,  /* function where the error happened */
    /* 200 */
    s_mount_opts: StaticLenString<64>,
    s_usr_quota_inum: u32,       /* inode for tracking user quota */
    s_grp_quota_inum: u32,       /* inode for tracking group quota */
    s_overhead_clusters: u32,    /* overhead blocks/clusters in fs */
    s_backup_bgs: [u32; 2],      /* groups with sparse_super2 SBs */
    s_encrypt_algos: [u8; 4],    /* Encryption algorithms in use  */
    s_encrypt_pw_salt: [u8; 16], /* Salt used for string2key algorithm */
    s_lpf_ino: u32,              /* Location of the lost+found inode */
    s_prj_quota_inum: u32,       /* inode for tracking project quota */
    s_checksum_seed: u32,        /* crc32c(uuid) if csum_seed set */
    s_wtime_hi: u8,
    s_mtime_hi: u8,
    s_mkfs_time_hi: u8,
    s_lastcheck_hi: u8,
    s_first_error_time_hi: u8,
    s_last_error_time_hi: u8,
    s_first_error_errcode: u8,
    s_last_error_errcode: u8,
    s_encoding: u16,                  /* Filename charset encoding */
    s_encoding_flags: u16,            /* Filename charset encoding flags */
    s_orphan_file_inum: u32,          /* Inode for tracking orphan inodes */
    s_reserved: [u8; 376] = [0; 376], /* Padding to the end of the block */
    s_checksum: u32, /* crc32c(superblock) */
}}
impl Ext4SuperBlock {
    pub fn new(uuid: [u8; 16], inodes_per_group: u32) -> Self {
        Ext4SuperBlock {
            s_blocks_per_group: 32768,
            s_clusters_per_group: 32768,
            s_inodes_per_group: inodes_per_group,
            s_mtime: 0,
            s_wtime: 1758215058,
            s_mnt_count: 0,
            s_max_mnt_count: 65535,
            s_magic: 0xef53,
            s_state: 1,
            s_errors: 1,
            s_minor_rev_level: 0,
            s_lastcheck: 1758215058,
            s_checkinterval: 0,
            s_rev_level: 1,
            s_def_resuid: 0,
            s_def_resgid: 0,
            s_first_ino: 11,
            s_inode_size: 256,
            s_block_group_nr: 0,
            s_feature_compat: 0x0038 | 0x0200,   /* sparse_super2 */
            s_feature_incompat: 0x02c2 | 0x8000, /* inline_data */
            s_feature_ro_compat: 0x046a,
            s_uuid: uuid,
            s_hash_seed: [940062939, 3880703204, 772543626, 1391354066],
            s_def_hash_version: 1,
            s_default_mount_opts: 0x000c,
            s_first_meta_bg: 0,
            s_mkfs_time: 1758215058,
            s_min_extra_isize: 32,
            s_want_extra_isize: 32,
            s_flags: 1,
            s_log_groups_per_flex: 4,
            s_kbytes_written: 9,
            ..Default::default()
        }
    }

    hi_lo_field_u64!(
        blocks_count,
        set_blocks_count,
        s_blocks_count_hi,
        s_blocks_count_lo
    );
    hi_lo_field_u64!(
        free_blocks_count,
        set_free_blocks_count,
        s_free_blocks_count_hi,
        s_free_blocks_count_lo
    );
    pub fn set_free_inodes_count(&mut self, count: u32) {
        self.s_free_inodes_count = count;
    }

    pub fn set_reserved_gdt_blocks(&mut self, count: u16) {
        self.s_reserved_gdt_blocks = count;
    }

    pub fn update_blocks_count(&mut self, count: u64) {
        self.set_blocks_count(count);
        self.s_inodes_count = self.block_groups_count() * self.inodes_per_group();
    }

    pub fn inodes_per_group(&self) -> u32 {
        self.s_inodes_per_group
    }

    pub fn block_groups_count(&self) -> u32 {
        let blocks_count = self.blocks_count() as u32;
        let blocks_per_group = self.s_blocks_per_group;
        blocks_count.div_ceil(blocks_per_group)
    }

    #[cfg(test)]
    pub fn uuid(&self) -> &[u8; 16] {
        &self.s_uuid
    }

    pub fn update_checksum(&mut self) {
        self.s_checksum = calculate_checksum![&self.as_bytes()[0..1020]];
    }
}

buffer_struct! { Ext4BlockGroupDescriptor {
    bg_block_bitmap_lo: u32,      /* Blocks bitmap block */
    bg_inode_bitmap_lo: u32,      /* Inodes bitmap block */
    bg_inode_table_lo: u32,       /* Inodes table block */
    bg_free_blocks_count_lo: u16, /* Free blocks count */
    bg_free_inodes_count_lo: u16, /* Free inodes count */
    bg_used_dirs_count_lo: u16,   /* Directories count */
    bg_flags: u16,                /* EXT4_BG_flags (INODE_UNINIT, etc) */
    bg_exclude_bitmap_lo: u32,    /* Exclude bitmap for snapshots */
    bg_block_bitmap_csum_lo: u16, /* crc32c(s_uuid+grp_num+bbitmap) LE */
    bg_inode_bitmap_csum_lo: u16, /* crc32c(s_uuid+grp_num+ibitmap) LE */
    bg_itable_unused_lo: u16,     /* Unused inodes count */
    bg_checksum: u16,             /* crc16(sb_uuid+group+desc) */
    bg_block_bitmap_hi: u32,      /* Blocks bitmap block MSB */
    bg_inode_bitmap_hi: u32,      /* Inodes bitmap block MSB */
    bg_inode_table_hi: u32,       /* Inodes table block MSB */
    bg_free_blocks_count_hi: u16, /* Free blocks count MSB */
    bg_free_inodes_count_hi: u16, /* Free inodes count MSB */
    bg_used_dirs_count_hi: u16,   /* Directories count MSB */
    bg_itable_unused_hi: u16,     /* Unused inodes count MSB */
    bg_exclude_bitmap_hi: u32,    /* Exclude bitmap block MSB */
    bg_block_bitmap_csum_hi: u16, /* crc32c(s_uuid+grp_num+bbitmap) BE */
    bg_inode_bitmap_csum_hi: u16, /* crc32c(s_uuid+grp_num+ibitmap) BE */
    bg_reserved: u32,
} }
impl Ext4BlockGroupDescriptor {
    hi_lo_field_u64!(
        block_bitmap,
        set_block_bitmap,
        bg_block_bitmap_hi,
        bg_block_bitmap_lo
    );
    hi_lo_field_u64!(
        inode_bitmap,
        set_inode_bitmap,
        bg_inode_bitmap_hi,
        bg_inode_bitmap_lo
    );
    hi_lo_field_u64!(
        inode_table,
        set_inode_table,
        bg_inode_table_hi,
        bg_inode_table_lo
    );
    hi_lo_field_u32!(
        block_bitmap_csum,
        set_block_bitmap_csum,
        bg_block_bitmap_csum_hi,
        bg_block_bitmap_csum_lo
    );
    hi_lo_field_u32!(
        inode_bitmap_csum,
        set_inode_bitmap_csum,
        bg_inode_bitmap_csum_hi,
        bg_inode_bitmap_csum_lo
    );
    hi_lo_field_u32!(
        free_blocks_count,
        set_free_blocks_count,
        bg_free_blocks_count_hi,
        bg_free_blocks_count_lo
    );
    hi_lo_field_u32!(
        free_inodes_count,
        set_free_inodes_count,
        bg_free_inodes_count_hi,
        bg_free_inodes_count_lo
    );
    hi_lo_field_u32!(
        used_dirs_count,
        set_used_dirs_count,
        bg_used_dirs_count_hi,
        bg_used_dirs_count_lo
    );

    pub fn update_checksums(
        &mut self,
        uuid: &[u8; 16],
        n: u32,
        block_bitmap: &BitmapBlock,
        inode_bitmap: &BitmapBlock,
    ) {
        self.set_block_bitmap_csum(calculate_checksum![uuid, &block_bitmap.data]);
        self.set_inode_bitmap_csum(calculate_checksum![
            uuid,
            &inode_bitmap.data[0..inode_bitmap.len.div_ceil(8) as usize]
        ]);
        self.bg_checksum = calculate_checksum!(uuid, &n.to_le_bytes(), &self.as_bytes()) as u16;
    }
}

pub struct BitmapBlock {
    data: [u8; 4096],
    len: u32,
}
impl BitmapBlock {
    pub fn from_bytes(data: &[u8], len: u32) -> Self {
        assert!(len <= 4096 * 8);
        let mut block = BitmapBlock {
            data: [0u8; 4096],
            len,
        };
        block.data[0..data.len()].copy_from_slice(data);
        for i in len..(4096 * 8) {
            block.set_bit(i);
        }
        block
    }
    pub fn set_bit(&mut self, n: u32) {
        let byte = (n / 8) as usize;
        let bit = n % 8;
        self.data[byte] |= 1 << bit;
    }
    pub fn free_count(&self) -> u32 {
        let mut count = 0;
        for i in 0..self.len {
            let byte = (i / 8) as usize;
            let bit = i % 8;
            if (self.data[byte] & (1 << bit)) == 0 {
                count += 1;
            }
        }
        count
    }
}
impl Debug for BitmapBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BitmapBlock {{\n    ")?;
        for i in 0..self.len {
            let byte = (i / 8) as usize;
            let bit = i % 8;
            if (self.data[byte] & (1 << bit)) != 0 {
                write!(f, "1")?;
            } else {
                write!(f, "0")?;
            }
            if i % 8 == 7 {
                write!(f, " ")?;
            }
            if i % 128 == 127 {
                writeln!(f)?;
                if i != self.len - 1 {
                    write!(f, "    ")?;
                }
            }
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}
impl Buffer<4096> for BitmapBlock {
    fn read_buffer(buf: &[u8]) -> Self {
        let mut data = [0u8; 4096];
        data.copy_from_slice(&buf[0..4096]);
        BitmapBlock { data, len: 4096 }
    }

    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0..4096].copy_from_slice(&self.data);
    }
}

buffer_struct! { Ext4Inode {
    i_mode: u16,               /* File mode */
    i_uid: u16,                /* Low 16 bits of Owner Uid */
    i_size_lo: u32,            /* Size in bytes */
    i_atime: u32,              /* Access time */
    i_ctime: u32,              /* Inode Change time */
    i_mtime: u32,              /* Modification time */
    i_dtime: u32,              /* Deletion Time */
    i_gid: u16,                /* Low 16 bits of Group Id */
    i_links_count: u16,        /* Links count */
    i_blocks_lo: u32,          /* Blocks count */
    i_flags: u32,              /* File flags */
    l_i_version: u32,          /* OS dependent 1 */
    i_block: [u8; 60] = [0; 60], /* Pointers to blocks */
    i_generation: u32,         /* File version (for NFS) */
    i_file_acl_lo: u32,        /* File ACL */
    i_size_high: u32,
    i_obso_faddr: u32,  /* Obsoleted fragment address */
    i_blocks_high: u16, /* were l_i_reserved1 */
    i_file_acl_high: u16,
    i_uid_high: u16,    /* these 2 fields */
    i_gid_high: u16,    /* were reserved2[0] */
    i_checksum_lo: u16, /* crc32c(uuid+inum+inode) LE */
    l_i_reserved: u16,
    i_extra_isize: u16 = 32,
    i_checksum_hi: u16,  /* crc32c(uuid+inum+inode) BE */
    i_ctime_extra: u32,  /* extra Change time      (nsec << 2 | epoch) */
    i_mtime_extra: u32,  /* extra Modification time(nsec << 2 | epoch) */
    i_atime_extra: u32,  /* extra Access time      (nsec << 2 | epoch) */
    i_crtime: u32,       /* File Creation time */
    i_crtime_extra: u32, /* extra FileCreationtime (nsec << 2 | epoch) */
    i_version_hi: u32,   /* high 32 bits for 64-bit version */
    i_projid: u32,       /* Project ID */
    rest: [u8; 96] = [0; 96],
} }
impl Ext4Inode {
    pub fn new(size: u64, extents: impl Buffer<60>, ty: FileType) -> Self {
        let mut inode = Ext4Inode::default();
        inode.set_file_type(ty);
        inode.i_links_count = 1;
        inode.update_size(size);
        extents.write_buffer(&mut inode.i_block);
        inode.i_flags = 0x80000; // EXT4_EXTENTS_FLAG
        inode
    }
    hi_lo_field_u64!(size, set_size, i_size_high, i_size_lo);
    hi_lo_field_u48!(blocks, set_blocks, i_blocks_high, i_blocks_lo);
    hi_lo_field_u32!(checksum, set_checksum, i_checksum_hi, i_checksum_lo);

    pub const MAX_INLINE_SIZE_BLOCK: usize = 60; // 60 bytes in i_block
    pub const MAX_INLINE_SIZE_XATTR: usize = 96 - Ext4ExtAttrEntryData::SIZE as usize - 4 - 4; // rest - xattr header
    pub const MAX_INLINE_SIZE: usize = Self::MAX_INLINE_SIZE_BLOCK + Self::MAX_INLINE_SIZE_XATTR;
    pub fn with_inline_data(block_data: &[u8], xattr_data: &[u8], ty: FileType) -> Self {
        let mut inode = Ext4Inode::default();

        inode.set_file_type(ty);
        inode.i_links_count = 1;
        inode.set_size((block_data.len() + xattr_data.len()) as u64);

        assert!(block_data.len() <= Self::MAX_INLINE_SIZE_BLOCK);
        assert!(xattr_data.len() <= Self::MAX_INLINE_SIZE_XATTR);
        if block_data.len() < inode.i_block.len() {
            assert!(xattr_data.is_empty());
        }

        inode.i_flags |= 0x10000000; // EXT4_INLINE_DATA_FL
        inode.i_block[..block_data.len()].copy_from_slice(block_data);

        let xattr_magic: u32 = 0xEA020000;
        inode.rest[0..4].copy_from_slice(&xattr_magic.to_le_bytes());
        let xattr = Ext4ExtAttrEntryData {
            e_value_offs: (Ext4ExtAttrEntryData::SIZE + 4).try_into().unwrap(),
            e_value_size: xattr_data.len().try_into().unwrap(),
            ..Default::default()
        };
        xattr.write_buffer(&mut inode.rest[4..]);
        let offset = 4 + 4 + Ext4ExtAttrEntryData::SIZE as usize;
        inode.rest[offset..(offset + xattr_data.len())].copy_from_slice(xattr_data);

        inode
    }

    pub fn update_size(&mut self, size: u64) {
        self.set_size(size);
        let blocks = size.div_ceil(BLOCK_SIZE);
        self.set_blocks(blocks * 8); // TODO: is this correct?
    }

    pub fn update_checksum(&mut self, uuid: &[u8; 16], n: u32) {
        self.set_checksum(0);
        self.set_checksum(calculate_checksum![
            uuid,
            &n.to_le_bytes(),
            &self.i_generation.to_le_bytes(),
            &self.as_bytes()
        ]);
        let ext4_inode_csum_hi_extra_end = 18;
        let has_hi = self.i_extra_isize >= ext4_inode_csum_hi_extra_end;
        if !has_hi {
            self.i_checksum_hi = 0;
        }
    }

    pub fn block_mut(&mut self) -> &mut [u8] {
        &mut self.i_block
    }
    pub fn set_links_count(&mut self, count: u16) {
        self.i_links_count = count
    }
    pub fn set_mode(&mut self, mode: u16) {
        self.i_mode = (self.i_mode & 0xf000) | (mode & 0x0fff);
    }
    pub fn set_file_type(&mut self, file_type: FileType) {
        self.i_mode = (self.i_mode & 0x0fff) | file_type.as_mode();
    }
    pub fn is_directory(&self) -> bool {
        (self.i_mode & 0xf000) == FileType::Directory.as_mode()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Null,
    Fifo,
    CharacterDevice,
    Directory,
    BlockDevice,
    RegularFile,
    SymbolicLink,
    Socket,
}
impl FileType {
    pub fn as_mode(&self) -> u16 {
        match self {
            FileType::Null => 0,
            FileType::Fifo => 0x1000,            // S_IFifo
            FileType::CharacterDevice => 0x2000, // S_IFCHR
            FileType::Directory => 0x4000,       // S_IFDIR
            FileType::BlockDevice => 0x6000,     // S_IFBLK
            FileType::RegularFile => 0x8000,     // S_IFREG
            FileType::SymbolicLink => 0xA000,    // S_IFLNK
            FileType::Socket => 0xC000,          // S_IFSOCK
        }
    }
    pub fn as_directory_entry_type(&self) -> u8 {
        match self {
            FileType::Null => 0,
            FileType::RegularFile => 1,
            FileType::Directory => 2,
            FileType::CharacterDevice => 3,
            FileType::BlockDevice => 4,
            FileType::Fifo => 5,
            FileType::Socket => 6,
            FileType::SymbolicLink => 7,
        }
    }
}

buffer_struct! { Ext4ExtAttrEntryData {
    e_name_len: u8 = 4,	    /* length of name */
    e_name_index: u8 = 7,	/* attribute name index */
    e_value_offs: u16 = 20,	/* offset of the value relative to the first entry */
    e_value_inum: u32 = 0,	/* inode in which the value is stored */
    e_value_size: u32,	    /* size of attribute value */
    e_hash: u32 = 0,		/* hash value of name and value */
    e_name: [u8; 4] = [0x64, 0x61, 0x74, 0x61],	/* attribute name = "data" */
} }

buffer_struct! { LegacyBlockDescriptor {
    direct: [u32; 12],
    indirect: u32,
    double_indirect: u32,
    triple_indirect: u32,
}}
impl LegacyBlockDescriptor {
    pub fn new(double_indirect: u32) -> Self {
        LegacyBlockDescriptor {
            double_indirect,
            ..Default::default()
        }
    }
    pub fn maximum_addressable_size() -> u64 {
        let direct = 12 * BLOCK_SIZE;
        let indirect = (BLOCK_SIZE / 8) * BLOCK_SIZE;
        let double_indirect = (BLOCK_SIZE / 8) * (BLOCK_SIZE / 8) * BLOCK_SIZE;
        direct + indirect + double_indirect
    }
}

buffer_struct! { Ext4InlineExtents {
    header: Ext4ExtentHeader,
    extents: [Ext4ExtentLeafNode; 4],
} }
impl Ext4InlineExtents {
    pub const MAX_INLINE_BLOCKS: u64 = Ext4ExtentLeafNode::MAX_LEN as u64 * 4; // we can represent up to 4 extents, each with a maximum length of 65535 blocks
    pub fn new(allocation: Allocation) -> Self {
        let blocks = allocation.end - allocation.start;
        assert!(blocks <= Self::MAX_INLINE_BLOCKS);
        let extents_needed = blocks.div_ceil(Ext4ExtentLeafNode::MAX_LEN as u64);
        let mut extents = [Ext4ExtentLeafNode::default(); 4];
        for i in 0..extents_needed {
            let len = if i == extents_needed - 1 {
                u16::try_from(blocks - i * (Ext4ExtentLeafNode::MAX_LEN as u64)).unwrap()
            } else {
                Ext4ExtentLeafNode::MAX_LEN
            };
            let start = allocation.start + i * (Ext4ExtentLeafNode::MAX_LEN as u64);
            extents[i as usize].set_start(start);
            extents[i as usize].ee_len = len;
            extents[i as usize].ee_block = (i * (Ext4ExtentLeafNode::MAX_LEN as u64)) as u32;
        }

        Ext4InlineExtents {
            header: Ext4ExtentHeader {
                eh_entries: extents_needed.try_into().unwrap(),
                ..Default::default()
            },
            extents,
        }
    }

    #[cfg(test)]
    fn as_blocks_range(&self) -> std::ops::Range<u64> {
        assert_eq!(self.header.eh_entries, 1);
        assert_eq!(self.header.eh_depth, 0);
        self.extents[0].start()..(self.extents[0].start() + self.extents[0].ee_len as u64)
    }
}

buffer_struct! { Ext4IndirectExtents {
    header: Ext4ExtentHeader,
    extents: [Ext4ExtentInternalNode; 4],
} }
impl Ext4IndirectExtents {
    pub fn create_block(
        allocation: Allocation,
        inode_num: u32,
        fs_uuid: &[u8; 16],
    ) -> [u8; BLOCK_SIZE as usize] {
        let blocks = allocation.end - allocation.start;
        let extents_needed = blocks.div_ceil(Ext4ExtentLeafNode::MAX_LEN as u64);
        assert!(
            Ext4ExtentHeader::SIZE + extents_needed * Ext4ExtentLeafNode::SIZE + 4 /* checksum */
                <= BLOCK_SIZE
        );
        let mut buf = [0u8; BLOCK_SIZE as usize];
        let header = Ext4ExtentHeader {
            eh_entries: extents_needed.try_into().unwrap(),
            eh_max: ((BLOCK_SIZE - Ext4ExtentHeader::SIZE - 4) / Ext4ExtentLeafNode::SIZE) as u16,
            eh_depth: 1,
            ..Default::default()
        };
        header.write_buffer(&mut buf);
        for i in 0..extents_needed {
            let len = if i == extents_needed - 1 {
                u16::try_from(blocks - i * (Ext4ExtentLeafNode::MAX_LEN as u64)).unwrap()
            } else {
                Ext4ExtentLeafNode::MAX_LEN
            };
            let start = allocation.start + i * (Ext4ExtentLeafNode::MAX_LEN as u64);
            let mut extent = Ext4ExtentLeafNode::default();
            extent.ee_block = (i * (Ext4ExtentLeafNode::MAX_LEN as u64)) as u32;
            extent.ee_len = len;
            extent.set_start(start);
            let start_offset =
                Ext4ExtentHeader::SIZE as usize + i as usize * Ext4ExtentLeafNode::SIZE as usize;
            extent.write_buffer(&mut buf[start_offset..]);
        }
        let checksum_offset = BLOCK_SIZE as usize - 4;
        let inode_generation: u32 = 0;
        let checksum = calculate_checksum![
            fs_uuid,
            &inode_num.to_le_bytes(),
            &inode_generation.to_le_bytes(),
            &buf[0..checksum_offset]
        ];
        buf[checksum_offset..].copy_from_slice(&checksum.to_le_bytes());
        buf
    }

    pub fn new(block: u64) -> Self {
        let mut extents = [Ext4ExtentInternalNode::default(); 4];
        extents[0].set_leaf(block);
        Ext4IndirectExtents {
            header: Ext4ExtentHeader {
                eh_entries: 1,
                eh_depth: 1,
                ..Default::default()
            },
            extents,
        }
    }
}

buffer_struct! { Ext4ExtentHeader {
    eh_magic: u16 = 0xF30A,
    eh_entries: u16,        /* number of valid entries */
    eh_max: u16 = 4,        /* capacity of store in entries */
    eh_depth: u16,          /* has tree real underlying blocks? */
    eh_generation: u32 = 0, /* generation of the tree */
} }

buffer_struct! { Ext4ExtentInternalNode {
    ei_block: u32,      /* first logical block extent covers */
    ei_leaf_lo: u32,    /* Lower 32-bits of the block number of the extent node that is the next level lower in the tree. */
    ei_leaf_hi: u16,    /* high 16 bits of physical block */
    ei_unused: u16 = 0, /* low 32 bits of physical block */
} }
impl Copy for Ext4ExtentInternalNode {}
impl_buffer_for_array!(4, Ext4ExtentInternalNode, 12);
impl Ext4ExtentInternalNode {
    hi_lo_field_u48!(leaf, set_leaf, ei_leaf_hi, ei_leaf_lo);
}

buffer_struct! { Ext4ExtentLeafNode {
    ee_block: u32,    /* first logical block extent covers */
    ee_len: u16,      /* number of blocks covered by extent */
    ee_start_hi: u16, /* high 16 bits of physical block */
    ee_start_lo: u32, /* low 32 bits of physical block */
} }
impl Copy for Ext4ExtentLeafNode {}
impl_buffer_for_array!(4, Ext4ExtentLeafNode, 12);
impl Ext4ExtentLeafNode {
    pub const MAX_LEN: u16 = 32768; // sizes bigger than this signify uninitialized extents
    hi_lo_field_u48!(start, set_start, ee_start_hi, ee_start_lo);
}

buffer_struct! { Ext4DirEntryMeta {
    inode: u32,	   /* Inode number */
    rec_len: u16,  /* Directory entry length */
    name_len: u8,  /* Name length */
    file_type: u8, /* See file type macros EXT4_FT_* below */
} }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ext4DirEntry {
    meta: Ext4DirEntryMeta,
    name: String,
}
impl Ext4DirEntry {
    pub fn new(inode: u32, file_type: FileType, name: &str) -> Self {
        Ext4DirEntry {
            meta: Ext4DirEntryMeta {
                inode,
                rec_len: ((name.len() + 8).div_ceil(4) * 4).try_into().unwrap(), // align to 4 bytes
                name_len: name
                    .len()
                    .try_into()
                    .expect("directory entry names can at most be 255 bytes long"),
                file_type: file_type.as_directory_entry_type(),
            },
            name: String::from(name),
        }
    }
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut to_return = vec![0u8; self.meta.rec_len as usize];
        self.meta.write_buffer(&mut to_return);
        to_return
            [Ext4DirEntryMeta::SIZE as usize..(Ext4DirEntryMeta::SIZE as usize + self.name.len())]
            .copy_from_slice(self.name.as_bytes());
        to_return
    }
    pub fn is_directory(&self) -> bool {
        self.meta.file_type == FileType::Directory.as_directory_entry_type()
    }
    pub fn inode(&self) -> u32 {
        self.meta.inode
    }
    pub fn set_record_length(&mut self, rec_len: u16) {
        self.meta.rec_len = rec_len;
    }

    #[allow(dead_code)]
    pub fn read_buffer(buf: &[u8]) -> Self {
        let without_name = Ext4DirEntryMeta::read_buffer(buf);
        let name = String::from(
            std::str::from_utf8(&buf[8..(8 + without_name.name_len as usize)]).unwrap(),
        );
        Ext4DirEntry {
            meta: without_name,
            name,
        }
    }
}

buffer_struct! { Ext4DirEntryTail {
    det_reserved_zero: u32 = 0, /* Inode number, must be zero */
    det_rec_len: u16 = 12,      /* Directory entry length */
    det_reserved_zero2: u8 = 0, /* Name length, must be zero */
    det_reserved_ft: u8 = 0xDE, /* File type, must be 0xDE */
    det_checksum: u32,          /* Directory leaf block checksum. */
} }

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct LinearDirectoryBlock {
    entries: Vec<Ext4DirEntry>,
    checksum: u32,
}
impl LinearDirectoryBlock {
    pub fn update_checksum(&mut self, uuid: &[u8; 16], inode: u32, inode_generation: u32) {
        self.checksum = calculate_checksum![
            uuid,
            &inode.to_le_bytes(),
            &inode_generation.to_le_bytes(),
            &self.as_bytes()[0..4096 - 12]
        ];
    }
    pub fn fits(&self, entry: &Ext4DirEntry) -> bool {
        self.entries
            .iter()
            .map(|e: &Ext4DirEntry| e.meta.rec_len as usize)
            .sum::<usize>()
            + (entry.meta.rec_len as usize + Ext4DirEntryMeta::SIZE as usize)
            + Ext4DirEntryTail::SIZE as usize
            <= 4096
    }
    pub fn add_entry(&mut self, entry: Ext4DirEntry) {
        assert!(self.fits(&entry));
        self.entries.push(entry);
    }
}
impl Buffer<4096> for LinearDirectoryBlock {
    fn read_buffer(buf: &[u8]) -> Self {
        let mut entries = Vec::new();
        let mut offset = 0;
        while offset < 4096 - Ext4DirEntryTail::SIZE as usize {
            let entry = Ext4DirEntry::read_buffer(&buf[offset..]);
            offset += entry.meta.rec_len as usize;
            entries.push(entry);
        }
        let tail = Ext4DirEntryTail::read_buffer(&buf[4096 - 12..]);
        LinearDirectoryBlock {
            entries,
            checksum: tail.det_checksum,
        }
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        let mut offset = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            let mut entry = entry.clone();
            if i == self.entries.len() - 1 {
                entry.meta.rec_len = (4096 - 12 - offset).try_into().unwrap();
            }
            let entry_bytes = entry.as_bytes();
            buf[offset..(offset + entry_bytes.len())].copy_from_slice(&entry_bytes);
            offset += entry_bytes.len();
        }
        let tail = Ext4DirEntryTail {
            det_checksum: self.checksum,
            ..Default::default()
        };
        tail.write_buffer(&mut buf[4096 - 12..]);
    }
}

#[derive(Debug)]
pub struct InlineLinearDirectoryBlock {
    entries: Vec<Ext4DirEntry>,
    size: usize,
}
impl InlineLinearDirectoryBlock {
    pub fn new(size: usize) -> Self {
        InlineLinearDirectoryBlock {
            entries: Vec::new(),
            size,
        }
    }

    pub fn fits(&self, entry: &Ext4DirEntry) -> bool {
        self.entries
            .iter()
            .map(|e: &Ext4DirEntry| e.meta.rec_len as usize)
            .sum::<usize>()
            + (entry.meta.rec_len as usize + Ext4DirEntryMeta::SIZE as usize)
            <= self.size
    }
    pub fn add_entry(&mut self, entry: Ext4DirEntry) {
        assert!(self.fits(&entry));
        self.entries.push(entry);
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; self.size];
        if self.entries.is_empty() {
            let mut entry = Ext4DirEntry::new(0, FileType::Null, "");
            entry.set_record_length(self.size.try_into().unwrap());
            let entry_bytes = entry.as_bytes();
            buf[..entry_bytes.len()].copy_from_slice(&entry_bytes);
            return buf;
        }
        let mut offset = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            let mut entry = entry.clone();
            if i == self.entries.len() - 1 {
                entry.meta.rec_len = (self.size - offset).try_into().unwrap();
            }
            let entry_bytes = entry.as_bytes();
            buf[offset..(offset + entry_bytes.len())].copy_from_slice(&entry_bytes);
            offset += entry_bytes.len();
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        serialization::CheckMagic,
        util::{buffer_from_hexdump, hexdump},
    };
    use std::{
        fs,
        io::{Read, Seek},
        ops::Range,
    };

    #[test]
    fn test_static_len_str_str_len() {
        let s = StaticLenString::<16>::from_str("Hello, world!");
        assert_eq!(s.as_str(), "Hello, world!");
    }

    macro_rules! test_size_of {
        ($test_name:ident, $item:expr, $size:expr) => {
            #[test]
            fn $test_name() {
                let bytes = $item.as_bytes();
                assert_eq!(bytes.len(), $size);
            }
        };
    }

    test_size_of!(
        test_static_len_str_size_16,
        StaticLenString::<16>::default(),
        16
    );
    test_size_of!(test_superblock_size, Ext4SuperBlock::default(), 1024);
    test_size_of!(
        test_block_group_descriptor_size,
        Ext4BlockGroupDescriptor::default(),
        64
    );
    test_size_of!(
        test_block_bitmap_size,
        BitmapBlock::from_bytes(&[0u8; 4096], 128),
        4096
    );
    test_size_of!(test_single_extent_size, Ext4InlineExtents::default(), 60);
    test_size_of!(test_inode_size, Ext4Inode::default(), 256);
    test_size_of!(
        test_legacy_block_descriptor_size,
        LegacyBlockDescriptor::default(),
        60
    );
    test_size_of!(test_dir_entry_tail_size, Ext4DirEntryTail::default(), 12);

    #[test]
    fn test_read_inline_dir_inode() {
        let buf = buffer_from_hexdump(
            "
            0000  fd41 e803 3c00 0000 ee72 d868 8b1f d768  .A..<....r.h...h
            0020  8b1f d768 0000 0000 e803 0400 0000 0000  ...h............
            0040  0000 0010 0000 0000 0200 0000 0d00 0000  ................
            0060  1400 0c02 6c6f 6e67 6572 5f65 6e74 7279  ....longer_entry
            0100  0e00 0000 2400 0b02 7368 6f72 745f 656e  ....$...short_en
            0120  7472 7900 0000 0000 0000 0000 0000 0000  try.............
            0140  0000 0000 0000 0000 0000 0000 0000 0000  ................
            0160  0000 0000 0000 0000 0000 0000 bade 0000  ................
            0200  2000 75b0 0000 0000 0000 0000 0000 0000   .u.............
            0220  b77a d868 0000 0000 0000 0000 0000 0000  .z.h............
            0240  0000 02ea 0407 5c00 0000 0000 0000 0000  ......\\.........
            0260  0000 0000 6461 7461 0000 0000 0000 0000  ....data........
            0300  0000 0000 0000 0000 0000 0000 0000 0000  ................
            *
            0360  0000 0000 0000 0000 0000 0000 0000 0000  ................

        ",
        );
        let inode = Ext4Inode::read_buffer(&buf);
        dbg!(&inode);

        println!("{}", hexdump(&inode.i_block));
        println!("{}", hexdump(&inode.rest));
    }

    #[test]
    fn test_indirect_extents() {
        let buf = buffer_from_hexdump(
            "
            0000  0af3 0a00 5401 0000 0000 0000 0000 0000  ....T...........
            0020  d95a 0000 2725 0000 d95a 0000 ff7f 0000  .Z..'%...Z......
            0040  0185 0000 d8da 0000 007b 0000 0005 0100  .........{......
            0060  d855 0100 ff7f 0000 0185 0100 d7d5 0100  .U..............
            0100  0100 0000 0005 0200 d8d5 0100 fe7a 0000  .............z..
            0120  0205 0200 d650 0200 ff7f 0000 0185 0200  .....P..........
            0140  d5d0 0200 007b 0000 0005 0300 d54b 0300  .....{.......K..
            0160  ff7f 0000 0185 0300 d4cb 0300 2c34 0000  ............,4..
            0200  0005 0400 0000 0000 0000 0000 0000 0000  ................
            0220  0000 0000 0000 0000 0000 0000 0000 0000  ................
            *
            7760  0000 0000 0000 0000 0000 0000 dbcc c82d  ...............-
        ",
        );
        assert_eq!(buf.len(), BLOCK_SIZE as usize);
        let header = Ext4ExtentHeader::read_buffer(&buf);
        dbg!(&header);
        for i in 0..header.eh_entries {
            let start = Ext4ExtentHeader::SIZE as usize
                + i as usize * Ext4ExtentInternalNode::SIZE as usize;
            let extent = Ext4ExtentInternalNode::read_buffer(&buf[start..]);
            dbg!(&extent);
        }
        let checksum = u32::from_le_bytes(buf[BLOCK_SIZE as usize - 4..].try_into().unwrap());
        dbg!(checksum);
        let fs_uuid: [u8; _] = [
            220, 155, 229, 19, 223, 238, 78, 15, 153, 235, 134, 59, 35, 21, 141, 175,
        ];
        let inode_number = 12u32;
        let inode_generation = 0u32;
        let calculated_checksum = calculate_checksum![
            &fs_uuid,
            &inode_number.to_le_bytes(),
            &inode_generation.to_le_bytes(),
            &buf[0..BLOCK_SIZE as usize - 4]
        ];
        assert_eq!(checksum, calculated_checksum);
    }

    fn open_image() -> impl FnMut(Range<u64>) -> Vec<u8> {
        let image_path = "target/example.img";
        let stamp_path = "target/example.img.stamp";
        if !fs::exists(&image_path).unwrap() {
            std::process::Command::new("mkfs.ext4")
                .args(&[
                    "-d",
                    "src/",
                    "-O",
                    "inline_data",
                    "-b",
                    "4096",
                    image_path,
                    "1000",
                ])
                .output()
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            fs::write(stamp_path, []).unwrap()
        }
        while !fs::exists(stamp_path).unwrap() {
            // wait for the file to be fully written
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let mut file = fs::File::open(image_path).unwrap();
        move |range: Range<u64>| {
            file.seek(std::io::SeekFrom::Start(range.start)).unwrap();
            let mut buf = vec![0u8; (range.end - range.start) as usize];
            file.read_exact(&mut buf).unwrap();
            buf
        }
    }

    #[test]
    fn test_read_superblock() {
        let mut image = open_image();
        let mut sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        dbg!(&sb);
        sb.update_checksum();
        sb.check_magic().unwrap();
    }

    #[test]
    fn test_read_block_group_table() {
        let mut image = open_image();
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        sb.check_magic().unwrap();
        let no_of_block_groups = sb.blocks_count().div_ceil(sb.s_blocks_per_group as u64);
        for i in 0..no_of_block_groups as usize {
            let bgd: Ext4BlockGroupDescriptor = Ext4BlockGroupDescriptor::read_buffer(&image(
                (4096 + i * 256) as u64..(4096 + (i + 1) * 256) as u64,
            ));
            println!("{:#?}", bgd);
        }
    }

    #[test]
    fn test_read_inode_bitmap() {
        let mut image = open_image();
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        sb.check_magic().unwrap();
        let bgd: Ext4BlockGroupDescriptor =
            Ext4BlockGroupDescriptor::read_buffer(&image(4096..8192));
        let inode_bitmap_block = bgd.inode_bitmap();
        let inode_bitmap = BitmapBlock::read_buffer(&image(
            (inode_bitmap_block * BLOCK_SIZE) as u64
                ..((inode_bitmap_block + 1) * BLOCK_SIZE) as u64,
        ));
        println!("{inode_bitmap:#?}")
    }

    #[test]
    fn test_read_resize_inode() {
        let mut image = open_image();
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        sb.check_magic().unwrap();
        let bgd: Ext4BlockGroupDescriptor =
            Ext4BlockGroupDescriptor::read_buffer(&image(4096..8192));
        let inode_table_block = bgd.inode_table();
        let resize_inode_num = 7;
        let inode_offset = (resize_inode_num - 1) * 256;
        let mut inode: Ext4Inode = Ext4Inode::read_buffer(&image(
            (inode_table_block * BLOCK_SIZE + inode_offset) as u64
                ..(inode_table_block * BLOCK_SIZE + inode_offset + Ext4Inode::SIZE) as u64,
        ));
        let old_checksum = inode.checksum();
        inode.update_checksum(sb.uuid(), resize_inode_num as u32);
        assert_eq!(old_checksum, inode.checksum());
        println!("{:#?}", inode);
        dbg!(inode.size());

        let extent = LegacyBlockDescriptor::read_buffer(&inode.i_block);
        println!("{:#?}", extent);
        let block = extent.double_indirect;
        let block_map = &image(
            ((block as u64 + 0) * BLOCK_SIZE) as u64..((block as u64 + 2) * BLOCK_SIZE) as u64,
        );
        let block_map = <[u32; 1024]>::read_buffer(&block_map);
        println!("Indirect: {:?}", &block_map);
    }

    #[test]
    fn test_read_root_directory() {
        let mut image = open_image();
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        sb.check_magic().unwrap();
        let bgd: Ext4BlockGroupDescriptor =
            Ext4BlockGroupDescriptor::read_buffer(&image(4096..8192));
        let inode_table_block = bgd.inode_table();
        let root_dir_inode_num = 2;
        let inode_offset = (root_dir_inode_num - 1) * 256;
        let mut inode: Ext4Inode = Ext4Inode::read_buffer(&image(
            (inode_table_block * BLOCK_SIZE + inode_offset) as u64
                ..(inode_table_block * BLOCK_SIZE + inode_offset + Ext4Inode::SIZE) as u64,
        ));
        println!("{:#?}", inode);
        println!("{}", hexdump(&inode.block_mut()));
        println!("{}", hexdump(&inode.rest));

        let old_checksum = inode.checksum();
        inode.update_checksum(sb.uuid(), root_dir_inode_num as u32);
        assert_eq!(old_checksum, inode.checksum());

        let block = &inode.block_mut();
        let extent: Ext4InlineExtents = Ext4InlineExtents::read_buffer(block);
        extent.check_magic().unwrap();
        println!("{:#?}", extent);

        for block in extent.as_blocks_range() {
            dbg!(block);
            let block_data = &image((block * BLOCK_SIZE) as u64..((block + 1) * BLOCK_SIZE) as u64);
            let mut dir_block = LinearDirectoryBlock::read_buffer(block_data);
            let old_checksum = dir_block.checksum;
            dir_block.update_checksum(sb.uuid(), root_dir_inode_num as u32, inode.i_generation);
            assert_eq!(old_checksum, dir_block.checksum);
        }
    }

    #[test]
    fn test_read_file() {
        let mut image = open_image();
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_buffer(&image(1024..4096));
        sb.check_magic().unwrap();
        let bgd: Ext4BlockGroupDescriptor =
            Ext4BlockGroupDescriptor::read_buffer(&image(4096..8192));
        let inode_table_block = bgd.inode_table();
        let file_inode_num = 12;
        let inode_offset = (file_inode_num - 1) * 256;
        let mut inode: Ext4Inode = Ext4Inode::read_buffer(&image(
            (inode_table_block * BLOCK_SIZE + inode_offset)
                ..(inode_table_block * BLOCK_SIZE + inode_offset + Ext4Inode::SIZE),
        ));
        println!("{:#?}", inode);

        let old_checksum = inode.checksum();
        inode.update_checksum(sb.uuid(), file_inode_num as u32);
        assert_eq!(old_checksum, inode.checksum());

        let block = &inode.block_mut();
        let extent = Ext4IndirectExtents::read_buffer(block);
        println!("{:#?}", extent);
    }
}
