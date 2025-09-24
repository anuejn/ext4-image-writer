#[allow(dead_code)]
pub fn hexdump(data: &[u8]) {
    println!("Hexdump ({} bytes):", data.len());
    let mut last_omitted = false;
    for (i, chunk) in data.chunks(16).enumerate() {
        if chunk.iter().all(|&b| b == 0) {
            if !last_omitted {
                println!("*");
            }
            last_omitted = true;
            continue;
        }
        last_omitted = false;
        print!("{:08X}  ", i * 16);
        for byte in chunk {
            print!("{:02X} ", byte);
        }
        for _ in 0..(16 - chunk.len()) {
            print!("   ");
        }
        print!(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
    println!("Hexdump end.");
}
