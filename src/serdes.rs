use std::fmt::Debug;

pub trait Buffer<const N: usize> {
    const SIZE: usize = N;
    fn read_buffer(buf: &[u8]) -> Self;
    fn write_buffer(&self, buf: &mut [u8]);

    fn as_buffer(&self) -> [u8; N] {
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


#[derive(Clone)]
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

macro_rules! ext4_struct {
    ($name:ident { $( $it:ident : $value:ty $(= $default:expr)?, )* }) => {
        struct $name {
            $( $it: $value ),*
        }


        const fn buffer_size<const N: usize, T: Buffer<N>>() -> usize {
            N
        }

        impl Buffer<{0 $( + buffer_size::<_, $value>())*}> for $name {
            fn read_buffer(buf: &[u8]) -> Self {
                let mut ptr = 0;
                $( 
                    let $it = <$value>::read_buffer(&buf[ptr..{ptr += buffer_size::<_, $value>(); ptr}]); 
                )*
                Self {
                    $( $it, )*
                }
            }

            fn write_buffer(&self, buf: &mut [u8]) {
                let mut ptr = 0;
                $(
                    self.$it.write_buffer(&mut buf[ptr..{ptr += buffer_size::<_, $value>(); ptr}]); 
                )*
            }
        }
    };
    
}

ext4_struct! { TestStruct {
    a: u8 = 1,
    b: u16,
    c: u32 = 3,
    d: u64 = 4,
    e: [u8; 16] = [0; 16],
    f: StaticLenString<16> = StaticLenString::from_str("hello"),
}}