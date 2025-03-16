use std::io::Error;

use crate::player::PlayerConnection;

pub mod handshake;
pub mod login;

pub mod data {
    use std::io::{Error, ErrorKind};


    pub fn write_varint(buffer: &mut Vec<u8>, mut value: u32) {
        while value >= 0x80 {
            buffer.push(((value & 0x7F) | 0x80) as u8); // Add CONTINUE_BIT (0x80) if more bytes follow
            value >>= 7;
        }
        buffer.push((value & 0x7F) as u8); // Final byte without CONTINUE_BIT
    }
    
    pub fn write_string(buffer: &mut Vec<u8>, val: &str) {
        let length = val.len();
        write_varint(buffer, length as u32);

        buffer.extend_from_slice(val.as_bytes());
    }

    pub fn read_ushort(buffer: &Vec<u8>, position: &mut usize) -> Result<u16, Error> {
        if *position + 2 > buffer.len() {
            return Err(Error::new(ErrorKind::Other, "Not enough bytes"));
        }
    
        let result = u16::from_be_bytes([buffer[*position], buffer[*position + 1]]);
    
        *position += 2;

        Ok(result)
    }

    pub fn read_varint(buffer: &[u8], position: &mut usize) -> Result<u32, Error> {
        let mut value: u32 = 0;
        let mut shift: u32 = 0;
    
        loop {
            if *position >= buffer.len() {
                return Err(Error::new(ErrorKind::Other, "Buffer overflow"));
            }
    
            let current_byte = buffer[*position];
            *position += 1; // Move to the next byte
    
            value |= ((current_byte & 0x7F) as u32) << shift; // Extract 7 bits and add to value
            if (current_byte & 0x80) == 0 {
                break; // Stop if CONTINUE_BIT is not set
            }
    
            shift += 7;
    
            if shift >= 32 {
                return Err(Error::new(ErrorKind::Other, "Varint too big!"))?;
            }
        }
    
        Ok(value)
    }

    pub fn read_string(buffer: &[u8], position: &mut usize) -> Result<String, Error> {
        let length = read_varint(buffer, position)? as usize;
    
        if *position + length > buffer.len() {
            return Err(Error::new(ErrorKind::Other, "Buffer underflow whilst reading string"))?;
        }
    
        let string_bytes = &buffer[*position..*position + length];
        *position += length;
        match String::from_utf8(string_bytes.to_vec()) {
            Ok(string) => Ok(string),
            Err(_) => Err(Error::new(ErrorKind::Other, "Invalid UTF-8 in string"))?,
        }
    }
    
}

pub trait Packet: Clone {
    fn id() -> u32;
}

pub trait ProxyboundPacket: Packet { // (Player -> Proxy) -/-> Server 
    fn forward_to_proxy(&self);
    fn read_from(connection: &mut PlayerConnection) -> impl std::future::Future<Output = Result<Box<Self>, Error>> + Send;
}

pub trait PlayerboundPacket: Packet { // Server -> (Proxy -/-> Player)
    fn write_to(&self, buffer: &mut Vec<u8>);
}
