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
}

impl ProxyboundPacket for HandshakePacket {
    async fn read_from(connection: &mut PlayerConnection) -> Result<Box<HandshakePacket>, Error> {
        let (_, id, buffer) = connection.read_packet_raw().await?;
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

    fn forward_to_proxy(&self) {
        todo!()
    }
}
