use std::{fmt::Debug, io};

pub trait Buffer<const N: usize> {
    const SIZE: u64 = N as u64;

    #[allow(dead_code)]
    fn read_buffer(buf: &[u8]) -> Self;
    fn write_buffer(&self, buf: &mut [u8]);

    fn as_bytes(&self) -> [u8; N] {
        let mut buf = [0u8; N];
        self.write_buffer(&mut buf);
        buf
    }
}

impl Buffer<1> for u8 {
    fn read_buffer(buf: &[u8]) -> Self {
        buf[0]
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0] = *self;
    }
}
impl Buffer<2> for u16 {
    fn read_buffer(buf: &[u8]) -> Self {
        u16::from_le_bytes([buf[0], buf[1]])
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0..2].copy_from_slice(&self.to_le_bytes());
    }
}
impl Buffer<4> for u32 {
    fn read_buffer(buf: &[u8]) -> Self {
        u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.to_le_bytes());
    }
}
impl Buffer<8> for u64 {
    fn read_buffer(buf: &[u8]) -> Self {
        u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ])
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0..8].copy_from_slice(&self.to_le_bytes());
    }
}

impl<const N: usize> Buffer<N> for [u8; N] {
    fn read_buffer(buf: &[u8]) -> Self {
        let mut arr = [0u8; N];
        arr.copy_from_slice(&buf[0..N]);
        arr
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        buf[0..N].copy_from_slice(self);
    }
}
macro_rules! impl_buffer_for_u32_array {
    ($n:expr) => {
        impl Buffer<{ $n * 4 }> for [u32; $n] {
            fn read_buffer(buf: &[u8]) -> Self {
                let mut arr = [0u32; $n];
                for i in 0..$n {
                    arr[i] = u32::from_le_bytes([
                        buf[i * 4],
                        buf[i * 4 + 1],
                        buf[i * 4 + 2],
                        buf[i * 4 + 3],
                    ]);
                }
                arr
            }
            fn write_buffer(&self, buf: &mut [u8]) {
                for i in 0..$n {
                    let bytes = self[i].to_le_bytes();
                    buf[i * 4..i * 4 + 4].copy_from_slice(&bytes);
                }
            }
        }
    };
}
impl_buffer_for_u32_array!(2);
impl_buffer_for_u32_array!(4);
impl_buffer_for_u32_array!(12);
impl_buffer_for_u32_array!(17);
impl_buffer_for_u32_array!(1024);

macro_rules! impl_buffer_for_u64_array {
    ($n:expr) => {
        impl Buffer<{ $n * 8 }> for [u64; $n] {
            fn read_buffer(buf: &[u8]) -> Self {
                let mut arr = [0u64; $n];
                for i in 0..$n {
                    arr[i] = u64::from_le_bytes([
                        buf[i * 8],
                        buf[i * 8 + 1],
                        buf[i * 8 + 2],
                        buf[i * 8 + 3],
                        buf[i * 8 + 4],
                        buf[i * 8 + 5],
                        buf[i * 8 + 6],
                        buf[i * 8 + 7],
                    ]);
                }
                arr
            }
            fn write_buffer(&self, buf: &mut [u8]) {
                for i in 0..$n {
                    let bytes = self[i].to_le_bytes();
                    buf[i * 8..i * 8 + 8].copy_from_slice(&bytes);
                }
            }
        }
    };
}
impl_buffer_for_u64_array!(512);

#[derive(Clone, PartialEq, Eq)]
pub struct StaticLenString<const N: usize> {
    pub data: [u8; N],
}
impl<const N: usize> StaticLenString<N> {
    #[cfg(test)]
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
impl<const N: usize> Buffer<N> for StaticLenString<N> {
    fn read_buffer(buf: &[u8]) -> Self {
        StaticLenString {
            data: <[u8; N]>::read_buffer(buf),
        }
    }
    fn write_buffer(&self, buf: &mut [u8]) {
        self.data.write_buffer(buf);
    }
}

#[allow(dead_code)]
pub trait CheckMagic {
    fn check_magic(&self) -> io::Result<()>;
}

pub const fn buffer_size<const N: usize, T: Buffer<N>>() -> usize {
    N
}

macro_rules! ext4_struct {
    ($name:ident { $( $it:ident : $value:ty $(= $default:expr)?, )* }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            $( $it: $value ),*
        }

        impl Buffer<{0 $( + crate::serialization::buffer_size::<_, $value>())*}> for $name {
            fn read_buffer(buf: &[u8]) -> Self {
                let mut ptr = 0;
                $(
                    let $it = <$value>::read_buffer(&buf[ptr..{ptr += crate::serialization::buffer_size::<_, $value>(); ptr}]);
                )*
                Self {
                    $( $it, )*
                }
            }
            fn write_buffer(&self, buf: &mut [u8]) {
                let mut ptr = 0;
                $(
                    self.$it.write_buffer(&mut buf[ptr..{ptr += crate::serialization::buffer_size::<_, $value>(); ptr}]);
                )*
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self {$(
                    $it: ext4_struct!(generate_default $($default)?),
                )*}
            }
        }

        impl crate::serialization::CheckMagic for $name {
            fn check_magic(&self) -> std::io::Result<()> {
                $(
                    ext4_struct!(generate_check_magic self $it $($default)?);
                )*
                Ok(())
            }
        }
    };
    (generate_default $value:expr) => {
        $value
    };
    (generate_default) => {
        Default::default()
    };
    (generate_check_magic $self:ident $it:ident $value:expr) => {
        if $self.$it != $value {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid magic for field {}", stringify!($it))));
        }
    };
    (generate_check_magic $self:ident $it:ident) => { };
}
pub(crate) use ext4_struct;

macro_rules! hi_lo_field_u64 {
    ($get_name:ident, $set_name:ident, $hi:ident, $lo:ident) => {
        #[allow(dead_code)]
        pub fn $get_name(&self) -> u64 {
            ((self.$hi as u64) << 32) | (self.$lo as u64)
        }
        #[allow(dead_code)]
        pub fn $set_name(&mut self, value: u64) {
            self.$hi = (value >> 32) as u32;
            self.$lo = (value & 0xFFFFFFFF) as u32;
        }
    };
}
pub(crate) use hi_lo_field_u64;

macro_rules! hi_lo_field_u32 {
    ($get_name:ident, $set_name:ident, $hi:ident, $lo:ident) => {
        #[allow(dead_code)]
        pub fn $get_name(&self) -> u32 {
            ((self.$hi as u32) << 16) | (self.$lo as u32)
        }
        #[allow(dead_code)]
        pub fn $set_name(&mut self, value: u32) {
            self.$hi = (value >> 16) as u16;
            self.$lo = (value & 0xFFFF) as u16;
        }
    };
}
pub(crate) use hi_lo_field_u32;

macro_rules! hi_lo_field_u48 {
    ($get_name:ident, $set_name:ident, $hi:ident, $lo:ident) => {
        #[allow(dead_code)]
        pub fn $get_name(&self) -> u64 {
            ((self.$hi as u64) << 32) | (self.$lo as u64)
        }
        #[allow(dead_code)]
        pub fn $set_name(&mut self, value: u64) {
            self.$hi = (value >> 32) as u16;
            self.$lo = (value & 0xFFFFFFFF) as u32;
        }
    };
}
pub(crate) use hi_lo_field_u48;
