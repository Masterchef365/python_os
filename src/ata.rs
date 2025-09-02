//! https://wiki.osdev.org/ATA_PIO_Mode
use alloc::vec;
use alloc::vec::Vec;

use x86_64::instructions::port::{PortWriteOnly, PortReadOnly, Port};

use crate::println;

fn write_port_byte(port: u16, value: u8) {
    unsafe {
        PortWriteOnly::new(port).write(value);
    }
}

pub fn read_lba(drive_sel: bool, lba: u64, sectors: u16) -> Vec<u8> {
    const BLOCK_SIZE: usize = 512;
    let mut outbuf = Vec::with_capacity(sectors as usize * BLOCK_SIZE);

    let [lba1, lba2, lba3, lba4, lba5, lba6, _lba7, _lba8] = lba.to_le_bytes();
    let [sector_low, sector_high] = sectors.to_le_bytes();

    write_port_byte(0x1F6, 0x40 | ((drive_sel as u8) << 4));
    write_port_byte(0x1F2, sector_high);
    write_port_byte(0x1F3, lba4);
    write_port_byte(0x1F4, lba5);
    write_port_byte(0x1F5, lba6);
    write_port_byte(0x1F2, sector_low);
    write_port_byte(0x1F3, lba1);
    write_port_byte(0x1F4, lba2);
    write_port_byte(0x1F5, lba3);

    let mut command_port = Port::new(0x1F7);
    unsafe {
        command_port.write(0x24_u8); // READ SECTORS
        while command_port.read() & 8_u8 == 0 {}
    }

    let mut data_port = PortReadOnly::new(0x1F0);

    for _ in 0..256 {
        let value: u16 = unsafe { data_port.read() };
        let [low, high] = value.to_le_bytes();
        outbuf.push(low);
        outbuf.push(high);
    }

    outbuf
}


pub fn write_lba(drive_sel: bool, lba: u64, sectors: u16, data: &[u8]) {
    const BLOCK_SIZE: usize = 512;
    assert_eq!(data.len(), sectors as usize * BLOCK_SIZE);

    let [lba1, lba2, lba3, lba4, lba5, lba6, _lba7, _lba8] = lba.to_le_bytes();
    let [sector_low, sector_high] = sectors.to_le_bytes();

    write_port_byte(0x1F6, 0x40 | ((drive_sel as u8) << 4));
    write_port_byte(0x1F2, sector_high);
    write_port_byte(0x1F3, lba4);
    write_port_byte(0x1F4, lba5);
    write_port_byte(0x1F5, lba6);
    write_port_byte(0x1F2, sector_low);
    write_port_byte(0x1F3, lba1);
    write_port_byte(0x1F4, lba2);
    write_port_byte(0x1F5, lba3);
    //write_port_byte(0x1F7, 0x24);

    let mut command_port = Port::new(0x1F7);
    unsafe {
        command_port.write(0x34_u8); // WRITE SECTORS
        while command_port.read() & 8_u8 == 0 {}
    }

    let mut data_port = Port::new(0x1F0);

    for word in data.chunks_exact(2) {
        let word = u16::from_le_bytes([word[0], word[1]]);
        unsafe {
            data_port.write(word);
        }
    }
}
