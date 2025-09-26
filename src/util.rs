//! These functions use the same hexdump format as the debugfs utility from e2fsprogs.
//! The format is a bit weird: the addresses are octal while the data is in hex.

#[allow(dead_code)]
pub fn hexdump(data: &[u8]) -> String {
    let mut to_return = String::new();
    let mut last_omitted = false;
    for (i, chunk) in data.chunks(16).enumerate() {
        if chunk.iter().all(|&b| b == 0) {
            if !last_omitted {
                to_return.push_str("*\n");
            }
            last_omitted = true;
            continue;
        }
        last_omitted = false;
        to_return.push_str(&format!("{:04o}  ", i * 16));
        for (i, byte) in chunk.iter().enumerate() {
            to_return.push_str(&format!("{:02X}", byte));
            if i % 2 == 1 {
                to_return.push(' ');
            }
        }
        for i in 0..(16 - chunk.len()) {
            to_return.push_str("  ");
            if (chunk.len() + i) % 2 == 1 {
                to_return.push(' ');
            }
        }

        to_return.push_str("  ");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                to_return.push_str(&format!("{}", *byte as char));
            } else {
                to_return.push('.');
            }
        }
        to_return.push('\n');
    }
    to_return
}

#[allow(dead_code)]
pub fn buffer_from_hexdump(hexdump: &str) -> Vec<u8> {
    let mut buffer = Vec::new();
    for line in hexdump.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') {
            continue;
        }
        let addr_len = line.find(' ').unwrap();
        let addr = usize::from_str_radix(&line[..addr_len], 8).unwrap();
        let rest = &line[addr_len..].trim_start();
        if rest.starts_with('*') {
            continue;
        }
        for i in 0..8 {
            let part = &rest[i * 5..i * 5 + 4];
            if part.trim().is_empty() {
                break;
            }
            buffer.resize(addr + i * 2 + 2, 0);
            buffer[addr + i * 2] = u8::from_str_radix(&part[0..2], 16).unwrap();
            buffer[addr + i * 2 + 1] = u8::from_str_radix(&part[2..4], 16).unwrap();
        }
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hexdump() {
        let data = b"Hello, world!\nThis is a test of the hexdump function.\n";
        println!("{}", hexdump(data));
    }

    #[test]
    fn test_hexdump_roundtrip() {
        let data = b"Hello, world!\nThis is a test of the hexdump function.\n";
        let dump = hexdump(data);
        let buffer = buffer_from_hexdump(&dump);
        assert_eq!(data.to_vec(), buffer);
    }
}
