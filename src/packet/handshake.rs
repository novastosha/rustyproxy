use std::io::{Error, ErrorKind};

use crate::player::PlayerConnection;

use super::{data, Packet, ProxyboundPacket};

#[derive(Clone)]
pub struct HandshakePacket {
    pub protocol: u32,
    pub server_address: String,
    pub port: u16,
    pub next_state: u32,
}

impl Packet for HandshakePacket {
    fn id() -> u32 {
        0x00
    }
    async fn read_from(connection: &mut PlayerConnection) -> Result<Box<HandshakePacket>, Error> {
        let (_, id, buffer) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        let mut position = 0 as usize;

        Ok(Box::new(HandshakePacket {
            protocol: data::read_varint(&buffer, &mut position).unwrap(),
            server_address: data::read_string(&buffer, &mut position).unwrap(),
            port: data::read_ushort(&buffer, &mut position).unwrap(),
            next_state: data::read_varint(&buffer, &mut position).unwrap(),
        }))
    }
    
    fn write_to(&self, buffer: &mut Vec<u8>) {
        data::write_varint(buffer, self.protocol);
        data::write_string(buffer, &self.server_address);
        data::write_ushort(buffer, self.port);
        data::write_varint(buffer, self.next_state);
    }
}

impl ProxyboundPacket for HandshakePacket {}
