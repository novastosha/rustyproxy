use std::io::{Error, ErrorKind, Read, Write};

use azalea_chat::FormattedText;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::player::PlayerConnection;

pub mod handshake;
pub mod login;
pub mod play;
pub mod configuration;

pub mod data {
    use std::io::{Error, ErrorKind};

    use azalea_chat::FormattedText;
    use serde::Deserialize;
    use serde_json::Value;
    use uuid::Uuid;

    pub mod nbt {
        use std::io::{Cursor, Error};

        use azalea_chat::FormattedText;
        use crab_nbt::{serde::{de::{from_bytes_unnamed, from_cursor_unnamed}, ser::to_bytes_unnamed}, Nbt};

        pub fn write_text(buffer: &mut Vec<u8>, value: &FormattedText) {
            let mut text_as_bytes = to_bytes_unnamed(value).unwrap();
            buffer.extend_from_slice(&text_as_bytes);
        }

        pub fn read_text(buffer: &[u8], position: &mut usize) -> Result<FormattedText, Error>{
            let mut cursor = Cursor::new(buffer);
            cursor.set_position(*position as u64);
            
            Ok(from_cursor_unnamed::<FormattedText>(&mut cursor).unwrap())
        }
    }

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

    pub fn write_ushort(buffer: &mut Vec<u8>, value: u16) {
        buffer.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_uuid(buffer: &mut Vec<u8>, uuid: &Uuid) {
        let uuid_bytes = uuid.as_bytes();
        buffer.extend_from_slice(uuid_bytes);
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
                return Err(Error::new(ErrorKind::Other, "Not enough bytes"));
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

    pub fn varint_size(mut value: u32) -> usize {
        let mut size = 0;
        loop {
            size += 1;
            if value & !0x7F == 0 {
                break;
            }
            value >>= 7;
        }
        size
    }

    pub fn read_string(buffer: &[u8], position: &mut usize) -> Result<String, Error> {
        let length = read_varint(buffer, position)? as usize;

        if *position + length > buffer.len() {
            return Err(Error::new(
                ErrorKind::Other,
                "Not enoguh bytes",
            ))?;
        }

        let string_bytes = &buffer[*position..*position + length];
        *position += length;
        match String::from_utf8(string_bytes.to_vec()) {
            Ok(string) => Ok(string),
            Err(_) => Err(Error::new(ErrorKind::Other, "Invalid UTF-8 in string"))?,
        }
    }

    pub fn read_uuid(buffer: &[u8], position: &mut usize) -> Result<Uuid, Error> {
        if *position + 16 > buffer.len() {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "Not enough bytes",
            ));
        }

        let uuid_bytes: [u8; 16] = buffer[*position..*position + 16].try_into().unwrap();
        *position += 16;

        Ok(Uuid::from_bytes(uuid_bytes))
    }

    pub fn read_text(buffer: &[u8], position: &mut usize) -> Result<FormattedText, Error> {
        let j: Value = serde_json::from_str(
            &read_string(&buffer, position).unwrap()
        )
        .unwrap();
    
        Ok(FormattedText::deserialize(&j)?)
    }

    pub fn write_text(buffer: &mut Vec<u8>, text: &FormattedText) {
        write_string(buffer, &serde_json::to_string(text).unwrap())
    }

    pub fn write_bool(buffer: &mut Vec<u8>, value: bool) {
        buffer.push(if value { 1 } else { 0 });
    }
    
    pub fn read_bool(buffer: &Vec<u8>, position: &mut usize) -> Result<bool, Error> {
        if *position >= buffer.len() {
            return Err(Error::new(ErrorKind::Other, "Not enough bytes"));
        }
    
        let result = buffer[*position] != 0;
        *position += 1;
    
        Ok(result)
    }
}

pub type RawPacket = (u32, u32, Vec<u8>);
pub async fn read_packet(
    stream: &mut TcpStream,
    compression_threshold: u32,
) -> Result<RawPacket, Error> {
    // Read first byte to determine packet size
    let mut temp_buffer = vec![0u8; 5];
    stream.read_exact(&mut temp_buffer[..1]).await?;

    let mut position = 0;
    let mut slice = &temp_buffer[..];
    let packet_length = data::read_varint(&mut slice, &mut position)?;

    // Ensure the packet size is valid
    if packet_length == 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Packet length cannot be zero"));
    }

    // Read full packet into buffer
    let mut buffer = vec![0u8; packet_length as usize];
    stream.read_exact(&mut buffer).await?;

    let mut position = 0;
    let mut slice = &buffer[..];

    if compression_threshold == 0 {
        // No compression: Read packet ID and return
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();
        return Ok((packet_length, packet_id, data));
    }

    // Read Data Length
    let data_length = data::read_varint(&mut slice, &mut position)?;

    let (packet_id, data) = if data_length >= compression_threshold {
        // Compressed packet: Decompress safely
        let compressed_data = &slice[position..];
        let mut decoder = ZlibDecoder::new(compressed_data);
        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data)?;

        let mut position = 0;
        let mut slice = &decompressed_data[..];
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();

        (packet_id, data)
    } else {
        // Uncompressed packet
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();
        (packet_id, data)
    };

    Ok((packet_length, packet_id, data))
}

pub fn read_packet_from_bytes(mut slice: &[u8], compression_threshold: u32) -> Result<RawPacket, Error> {
    let mut position = 0;

    let packet_length = data::read_varint(&mut slice, &mut position)?;
    if slice.len() < packet_length as usize {
        return Err(Error::new(ErrorKind::UnexpectedEof, "Packet too short"));
    }

    if compression_threshold == 0 {
        // No compression: Read packet ID and return
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();
        return Ok((packet_length, packet_id, data));
    }

    // Read Data Length
    let data_length = data::read_varint(&mut slice, &mut position)?;

    let (packet_id, data) = if data_length >= compression_threshold {
        // Compressed packet: Decompress safely
        let compressed_data = &slice[position..];
        let mut decoder = ZlibDecoder::new(compressed_data);
        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data)?;

        let mut position = 0;
        let mut slice = &decompressed_data[..];
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();

        (packet_id, data)
    } else {
        // Uncompressed packet
        let packet_id = data::read_varint(&mut slice, &mut position)?;
        let data = slice[position..].to_vec();
        (packet_id, data)
    };

    Ok((packet_length, packet_id, data))
}

pub(crate) async fn send_packet<P: Packet>(
    packet: &P,
    cnx: &mut TcpStream,
    compression_threshold: u32,
) -> Result<(), Error> {
    let mut buffer = Vec::new();

    // Write packet ID first
    let packet_id = P::id();
    data::write_varint(&mut buffer, packet_id);

    // Write packet data
    let mut data_buffer = Vec::new();
    packet.write_to(&mut data_buffer);

    // Concatenate ID + data
    buffer.extend(data_buffer);
    let uncompressed_length = buffer.len() as u32;

    let mut final_buffer = Vec::new();

    if compression_threshold == 0 {
        // No compression - use original format
        data::write_varint(&mut final_buffer, uncompressed_length);
        final_buffer.extend(buffer);
    } else if uncompressed_length >= compression_threshold {
        // Compression is required
        println!("Compressing packet with ID: {}", P::id());

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&buffer)?; // Compress (Packet ID + Data)
        let compressed_data = encoder.finish()?;

        // Write total packet length (compressed data size + size of `Data Length`)
        let total_compressed_length =
            compressed_data.len() as u32 + data::varint_size(uncompressed_length) as u32;
        data::write_varint(&mut final_buffer, total_compressed_length);

        // Write uncompressed data length
        data::write_varint(&mut final_buffer, uncompressed_length);

        // Append compressed packet ID + Data
        final_buffer.extend(compressed_data);
    } else {
        // Packet size is below threshold, send uncompressed but in the new format
        println!("Sending uncompressed packet with ID: {}", P::id());
        
        let uncompressed_length_with_indicator =
            uncompressed_length + data::varint_size(0) as u32;

        // Write total packet length
        data::write_varint(&mut final_buffer, uncompressed_length_with_indicator);

        // Write Data Length = 0 (Uncompressed indicator)
        data::write_varint(&mut final_buffer, 0);

        // Append packet ID + Data
        final_buffer.extend(buffer);
    }

    // Send packet
    cnx.write_all(&final_buffer).await?;
    cnx.flush().await?;

    Ok(())
}

pub trait Packet: Clone {
    fn id() -> u32;

    fn write_to(&self, buffer: &mut Vec<u8>);
    fn read_from(
        connection: &mut PlayerConnection,
    ) -> impl std::future::Future<Output = Result<Box<Self>, Error>> + Send;
}

pub trait ProxyboundPacket: Packet {
    // (Player -> Proxy) -/-> Server
}

pub trait PlayerboundPacket: Packet {
    // Server -> (Proxy -/-> Player)
}
