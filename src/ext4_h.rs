use binrw::{BinRead, BinResult, BinWrite, binrw};
use std::fmt::Debug;
use std::io::{self, Seek};

use crate::{BLOCK_GROUP_SIZE, BLOCK_SIZE};

#[derive(BinRead, BinWrite, Clone)]
pub struct StaticLenString<const N: usize> {
    pub data: [u8; N],
}
impl<const N: usize> StaticLenString<N> {
    pub fn from_str(s: &str) -> Self {
        let mut data = [0u8; N];
        let bytes = s.as_bytes();
        let len = bytes.len().min(N);
        data[..len].copy_from_slice(&bytes[..len]);
        StaticLenString { data }
    }

    pub fn as_str(&self) -> &str {
        let len = self
            .data
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.data.len());
        std::str::from_utf8(&self.data[..len]).unwrap_or("")
    }
}
impl<const N: usize> Default for StaticLenString<N> {
    fn default() -> Self {
        StaticLenString { data: [0u8; N] }
    }
}
impl<const N: usize> Debug for StaticLenString<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StaticLenString::from_str(\"{}\")", self.as_str())
    }
}

pub struct Crc32cWriter<W: io::Write> {
    inner: W,
    checksum: u32,
}
impl<W: io::Write> io::Write for Crc32cWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.checksum = crc32c::crc32c_append(self.checksum, buf);
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<W: io::Write + io::Seek> Seek for Crc32cWriter<W> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        if let io::SeekFrom::Current(0) = pos {
            self.inner.seek(pos)
        } else {
            panic!("Seek other than Current(0) is not supported on Crc32cReader.");
        }
    }
}
impl<W: io::Write> Crc32cWriter<W> {
    pub fn new(writer: W) -> Self {
        Crc32cWriter {
            inner: writer,
            checksum: 0,
        }
    }
    pub fn crc32c(&self) -> u32 {
        self.checksum
    }
}

pub fn binwrite_as_buf<T: BinWrite>(value: &T) -> BinResult<Vec<u8>>
where
    for<'a> <T as BinWrite>::Args<'a>: Default,
{
    let mut buf = io::Cursor::new(Vec::new());
    value.write_le(&mut buf)?;
    Ok(buf.into_inner())
}

pub struct Crc32cReader<R: io::Read> {
    inner: R,
    checksum: u32,
}
impl<R: io::Read> io::Read for Crc32cReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.checksum = crc32c::crc32c_append(self.checksum, &buf[..n]);
        Ok(n)
    }
}
impl<R: io::Read + io::Seek> Seek for Crc32cReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        if let io::SeekFrom::Current(0) = pos {
            self.inner.seek(pos)
        } else {
            panic!("Seek other than Current(0) is not supported on Crc32cReader.");
        }
    }
}
impl<R: io::Read> Crc32cReader<R> {
    pub fn new(reader: R) -> Self {
        Crc32cReader {
            inner: reader,
            checksum: 0,
        }
    }
    pub fn crc32c(&self) -> u32 {
        self.checksum
    }
}

macro_rules! set_lo_hi {
    ($base:ident, $lo:tt, $hi:tt, $value:expr) => {
        $base.$lo = $value as u32;
        $base.$hi = ($value >> 32) as u32;
    };
}

#[binrw]
#[bw(little, stream = w, map_stream = Crc32cWriter::new)]
#[br(little, stream = r, map_stream = Crc32cReader::new)]
#[derive(Debug, Default)]
pub struct Ext4SuperBlock {
    /*00*/ s_inodes_count: u32,         /* Inodes count */
    s_blocks_count_lo: u32,      /* Blocks count */
    s_r_blocks_count_lo: u32,    /* Reserved blocks count */
    s_free_blocks_count_lo: u32, /* Free blocks count */
    /*10*/ s_free_inodes_count: u32, /* Free inodes count */
    s_first_data_block: u32,  /* First Data Block */
    s_log_block_size: u32,    /* Block size */
    s_log_cluster_size: u32,  /* Allocation cluster size */
    /*20*/ s_blocks_per_group: u32,   /* # Blocks per group */
    s_clusters_per_group: u32, /* # Clusters per group */
    s_inodes_per_group: u32,   /* # Inodes per group */
    s_mtime: u32,              /* Mount time */
    /*30*/ s_wtime: u32,           /* Write time */
    s_mnt_count: u16,       /* Mount count */
    s_max_mnt_count: u16,   /* Maximal mount count */
    s_magic: u16,           /* Magic signature */
    s_state: u16,           /* File system state */
    s_errors: u16,          /* Behaviour when detecting errors */
    s_minor_rev_level: u16, /* minor revision level */
    /*40*/ s_lastcheck: u32,     /* time of last check */
    s_checkinterval: u32, /* max. time between checks */
    s_creator_os: u32,    /* OS */
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
    s_desc_size: u16, /* size of group descriptor */
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
    s_checksum_type: u8,         /* metadata checksum algorithm used */
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
    s_reserved: StaticLenString<376>, /* Padding to the end of the block */
    #[br(temp, assert(r.crc32c() == 0xffffffff, "bad checksum: {:#08x} ({:#08x})", s_checksum, r.crc32c()))]
    #[bw(calc(0xffffffff - w.crc32c()))]
    s_checksum: u32, /* crc32c(superblock) */
}
impl Ext4SuperBlock {
    pub fn new() -> Self {
        Ext4SuperBlock {
            s_log_block_size: 2,
            s_log_cluster_size: 2,
            s_blocks_per_group: 32768,
            s_clusters_per_group: 32768,
            s_inodes_per_group: 4096,
            s_mtime: 0,
            s_wtime: 1758215058,
            s_mnt_count: 0,
            s_max_mnt_count: 65535,
            s_magic: 61267,
            s_state: 1,
            s_errors: 1,
            s_minor_rev_level: 0,
            s_lastcheck: 1758215058,
            s_checkinterval: 0,
            s_creator_os: 0,
            s_rev_level: 1,
            s_def_resuid: 0,
            s_def_resgid: 0,
            s_first_ino: 11,
            s_inode_size: 256,
            s_block_group_nr: 0,
            s_feature_compat: 56,
            s_feature_incompat: 706,
            s_feature_ro_compat: 1131,
            s_uuid: [
                213, 16, 84, 194, 97, 81, 76, 249, 133, 76, 213, 80, 197, 85, 78, 104,
            ],
            s_hash_seed: [940062939, 3880703204, 772543626, 1391354066],
            s_def_hash_version: 1,
            s_desc_size: 64,
            s_default_mount_opts: 12,
            s_first_meta_bg: 0,
            s_mkfs_time: 1758215058,
            s_min_extra_isize: 32,
            s_want_extra_isize: 32,
            s_flags: 1,
            s_log_groups_per_flex: 4,
            s_checksum_type: 1,
            s_kbytes_written: 9,
            ..Default::default()
        }
    }

    pub fn blocks_count(&self) -> u64 {
        (self.s_blocks_count_hi as u64) << 32 | (self.s_blocks_count_lo as u64)
    }

    pub fn inodes_per_group(&self) -> u32 {
        self.s_inodes_per_group as u32
    }

    pub fn block_groups_count(&self) -> u32 {
        let blocks_count = self.blocks_count() as u32;
        let blocks_per_group = self.s_blocks_per_group as u32;
        blocks_count.div_ceil(blocks_per_group)
    }

    pub fn with_blocks_count(mut self, blocks_count: u64) -> Self {
        set_lo_hi!(self, s_blocks_count_lo, s_blocks_count_hi, blocks_count);
        self.s_inodes_count = self.block_groups_count() * self.inodes_per_group();
        self
    }

    pub fn uuid(&self) -> &[u8; 16] {
        &self.s_uuid
    }
}

#[derive(Debug, BinRead, BinWrite, Default)]
pub struct Ext4BlockGroupDescriptor {
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
}
impl Ext4BlockGroupDescriptor {
    pub fn with_block_bitmap(mut self, block_bitmap: u64) -> Self {
        set_lo_hi!(self, bg_block_bitmap_lo, bg_block_bitmap_hi, block_bitmap);
        self
    }
    pub fn with_inode_bitmap(mut self, inode_bitmap: u64) -> Self {
        set_lo_hi!(self, bg_inode_bitmap_lo, bg_inode_bitmap_hi, inode_bitmap);
        self
    }
    pub fn with_inode_table(mut self, inode_table: u64) -> Self {
        set_lo_hi!(self, bg_inode_table_lo, bg_inode_table_hi, inode_table);
        self
    }
    pub fn with_checksums(
        mut self,
        uuid: &[u8; 16],
        n: u32,
        block_bitmap: &BitmapBlock,
        inode_bitmap: &BitmapBlock,
    ) -> Self {
        let block_bitmap_csum = 0;
        let block_bitmap_csum = crc32c::crc32c_append(block_bitmap_csum, uuid);
        let block_bitmap_csum = crc32c::crc32c_append(block_bitmap_csum, &block_bitmap.data);
        let block_bitmap_csum = 0xffff_ffff - block_bitmap_csum;
        self.bg_block_bitmap_csum_lo = (block_bitmap_csum & 0xffff) as u16;
        self.bg_block_bitmap_csum_hi = ((block_bitmap_csum >> 16) & 0xffff) as u16;

        let inode_bitmap_csum = 0;
        let inode_bitmap_csum = crc32c::crc32c_append(inode_bitmap_csum, uuid);
        let inode_bitmap_csum = crc32c::crc32c_append(
            inode_bitmap_csum,
            &inode_bitmap.data[0..inode_bitmap.len.div_ceil(8) as usize],
        );
        let inode_bitmap_csum = 0xffff_ffff - inode_bitmap_csum;
        self.bg_inode_bitmap_csum_lo = (inode_bitmap_csum & 0xffff) as u16;
        self.bg_inode_bitmap_csum_hi = ((inode_bitmap_csum >> 16) & 0xffff) as u16;

        self.bg_checksum = 0;
        let as_buf = binwrite_as_buf(&self).unwrap();
        let checksum = 0;
        let checksum = crc32c::crc32c_append(checksum, uuid);
        let checksum = crc32c::crc32c_append(checksum, &n.to_le_bytes());
        let checksum = crc32c::crc32c_append(checksum, &as_buf);
        self.bg_checksum = ((0xffffffff - checksum) & 0xffff) as u16;

        self
    }
}

#[binrw]
#[derive(Debug)]
pub struct BitmapBlock {
    data: [u8; 4096],
    #[brw(ignore)]
    len: u32,
}
impl BitmapBlock {
    pub fn new(len: u32) -> Self {
        BitmapBlock {
            data: [0u8; 4096],
            len,
        }
    }
}

macro_rules! ext4_struct {
    ($name:ident { $( $it:ident : $value:ty $(= $default:expr)?, )* }) => {
        struct $name {
            $( $it: $value ),*
        }

        impl $name {
            pub fn size() -> usize {
                0 $( + std::mem::size_of::<$value>())*
            }
        }
    };
    
}

ext4_struct! { 
    Ext4Inode2 {
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
        i_block: Ext4SingleExtent, /* Pointers to blocks */
        i_generation: u32,         /* File version (for NFS) */
        i_file_acl_lo: u32,        /* File ACL */
        i_size_high: u32,
        i_obso_faddr: u32,    /* Obsoleted fragment address */
        i_blocks_high: u16, /* were l_i_reserved1 */
        i_file_acl_high: u16,
        i_uid_high: u16,    /* these 2 fields */
        i_gid_high: u16,    /* were reserved2[0] */
        i_checksum_lo: u16 = self::checksum() as u16, /* crc32c(uuid+inum+inode) LE */
        l_i_reserved: u16,
        i_extra_isize: u16,
        i_checksum_hi: u16 = self::checksum() >> 16 as u16,  /* crc32c(uuid+inum+inode) BE */
        i_ctime_extra: u32,  /* extra Change time      (nsec << 2 | epoch) */
        i_mtime_extra: u32,  /* extra Modification time(nsec << 2 | epoch) */
        i_atime_extra: u32,  /* extra Access time      (nsec << 2 | epoch) */
        i_crtime: u32,       /* File Creation time */
        i_crtime_extra: u32, /* extra FileCreationtime (nsec << 2 | epoch) */
        i_version_hi: u32,   /* high 32 bits for 64-bit version */
        i_projid: u32,       /* Project ID */
        rest: Ext4InodeRest,
    }
}

#[derive(Debug, BinRead, BinWrite, Default, Clone)]

pub struct Ext4Inode {
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
    i_block: Ext4SingleExtent, /* Pointers to blocks */
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
    i_extra_isize: u16,
    i_checksum_hi: u16,  /* crc32c(uuid+inum+inode) BE */
    i_ctime_extra: u32,  /* extra Change time      (nsec << 2 | epoch) */
    i_mtime_extra: u32,  /* extra Modification time(nsec << 2 | epoch) */
    i_atime_extra: u32,  /* extra Access time      (nsec << 2 | epoch) */
    i_crtime: u32,       /* File Creation time */
    i_crtime_extra: u32, /* extra FileCreationtime (nsec << 2 | epoch) */
    i_version_hi: u32,   /* high 32 bits for 64-bit version */
    i_projid: u32,       /* Project ID */
    rest: Ext4InodeRest,
}
impl Ext4Inode {
    pub fn new(size: u64, offset: u64) -> Self {
        let mut inode = Ext4Inode::default();
        set_lo_hi!(inode, i_size_lo, i_size_high, size);
        let blocks = size.div_ceil(BLOCK_SIZE);
        inode.i_blocks_lo = blocks as u32;
        inode.i_block = Ext4SingleExtent::new(offset, inode.i_blocks_lo as u16);
        inode
    }
    pub fn offset(&self) -> u64 {
        (self.i_block.extent.ee_start_hi as u64) << 32 | (self.i_block.extent.ee_start_lo as u64)
    }
    pub fn block_group(&self) -> u32 {
        (self.offset() / BLOCK_GROUP_SIZE) as u32
    }
    pub fn with_checksum(mut self, uuid: &[u8; 16], n: u32) -> Self {
        let as_buf = binwrite_as_buf(&self).unwrap();
        let checksum = 0;
        let checksum = crc32c::crc32c_append(checksum, uuid);
        let checksum = crc32c::crc32c_append(checksum, &n.to_le_bytes());
        let checksum = crc32c::crc32c_append(checksum, &self.i_generation.to_le_bytes());
        let checksum = crc32c::crc32c_append(checksum, &as_buf);
        let checksum = 0xffff_ffff - checksum;
        self.i_checksum_lo = checksum as u16;
        self.i_checksum_hi = (checksum >> 16) as u16;
        self
    }
}

#[derive(Debug, BinRead, BinWrite, Default, Clone)]
struct Ext4InodeRest {
    padding: StaticLenString<96>,
}

#[derive(Debug, BinRead, BinWrite, Default, Clone)]
struct Ext4SingleExtent {
    // 60 bytes
    header: Ext4ExtentHeader,     // 12 bytes
    extent: Ext4Extent,           // 12 bytes
    padding: StaticLenString<36>, // 36 bytes
}
impl Ext4SingleExtent {
    pub fn new(block: u64, len: u16) -> Self {
        Ext4SingleExtent {
            header: Ext4ExtentHeader::default(),
            extent: Ext4Extent::new(block, len),
            padding: StaticLenString::default(),
        }
    }
}

#[binrw]
#[derive(Debug, Default, Clone)]
struct Ext4ExtentHeader {
    #[br(assert(eh_magic == 0, "bad extent header magic"))]
    #[bw(calc(0))]
    eh_magic: u16, /* probably will support different formats */
    #[br(assert(eh_magic == 0, "only one extent supported"))]
    #[bw(calc(0))]
    eh_entries: u16, /* number of valid entries */
    eh_max: u16, /* capacity of store in entries */
    #[br(assert(eh_magic == 0, "expecting leaf node"))]
    #[bw(calc(0))]
    eh_depth: u16, /* has tree real underlying blocks? */
    eh_generation: u32, /* generation of the tree */
}

#[derive(Debug, BinRead, BinWrite, Default, Clone)]
struct Ext4Extent {
    ee_block: u32,    /* first logical block extent covers */
    ee_len: u16,      /* number of blocks covered by extent */
    ee_start_hi: u16, /* high 16 bits of physical block */
    ee_start_lo: u32, /* low 32 bits of physical block */
}
impl Ext4Extent {
    pub fn new(block: u64, len: u16) -> Self {
        Ext4Extent {
            ee_block: block as u32,
            ee_len: len,
            ee_start_lo: block as u32,
            ee_start_hi: (block >> 32) as u16,
        }
    }
}

#[derive(Debug, BinRead, BinWrite, Default)]

struct Ext4DirEntry2 {
    inode: u32,   /* Inode number */
    rec_len: u16, /* Directory entry length */
    name_len: u8, /* Name length */
    file_type: u8,
    name: StaticLenString<255>,
}

#[derive(Debug, BinRead, BinWrite, Default)]

struct Ext4DxRoot {
    dot: FakeDirent,
    dot_name: StaticLenString<4>,
    dotdot: FakeDirent,
    dotdot_name: StaticLenString<4>,
    info: DxRootInfo,
    entries: [DxEntry; 0],
}

#[derive(Debug, BinRead, BinWrite, Default)]
struct DxRootInfo {
    reserved_zero: u32,
    hash_version: u8,
    info_length: u8, /* 8 */
    indirect_levels: u8,
    unused_flags: u8,
}

#[derive(Debug, BinRead, BinWrite, Default)]
struct FakeDirent {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    file_type: u8,
}

#[derive(Debug, BinRead, BinWrite, Default)]
struct DxEntry {
    hash: u32,
    block: u32,
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor};

    use super::*;

    #[test]
    fn test_static_len_str_str_len() {
        let s = StaticLenString::<16>::from_str("Hello, world!");
        assert_eq!(s.as_str(), "Hello, world!");
    }

    macro_rules! test_size_of {
        ($test_name:ident, $item:expr, $size:expr) => {
            #[test]
            fn $test_name() {
                let x = $item;
                let mut bytes = Cursor::new(Vec::new());
                x.write_le(&mut bytes).unwrap();
                assert_eq!(bytes.get_ref().len(), $size);
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
    test_size_of!(test_block_bitmap_size, BitmapBlock::new(128), 4096);
    test_size_of!(test_single_extent_size, Ext4SingleExtent::default(), 60);
    test_size_of!(test_inode_size, Ext4Inode::default(), 256);

    #[test]
    fn test_read_superblock() {
        // read data from test.img
        let data = fs::read("test.img").unwrap();
        let mut cursor = Cursor::new(data);
        cursor.set_position(1024); // superblock starts at offset 1024
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_le(&mut cursor).unwrap();
        assert_eq!(sb.s_magic, 0xEF53);
    }

    #[test]
    fn test_read_block_group_table() {
        let data = fs::read("target/smoke.img").unwrap();
        let mut cursor = Cursor::new(data);
        cursor.set_position(1024);
        let sb: Ext4SuperBlock = Ext4SuperBlock::read_le(&mut cursor).unwrap();
        assert_eq!(sb.s_magic, 0xEF53);
        let no_of_block_groups = sb.blocks_count().div_ceil(sb.s_blocks_per_group as u64);
        cursor.set_position(4096);
        for _ in 0..no_of_block_groups {
            let bgd: Ext4BlockGroupDescriptor =
                Ext4BlockGroupDescriptor::read_le(&mut cursor).unwrap();
            println!("{:#?}", bgd);
        }
    }
}
