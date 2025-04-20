use std::io::{Error, ErrorKind};

use azalea_chat::FormattedText;
use serde::Deserialize;
use serde_json::{self, Value};
use uuid::Uuid;

use crate::player::{PlayerConnection, PlayerInfo};

use super::{data, Packet, PlayerboundPacket, ProxyboundPacket};

#[derive(Clone)]
pub struct LoginDisconnectPacket {
    pub reason: FormattedText, // Perhaps there is a library for text components
}

impl Packet for LoginDisconnectPacket {
    fn id() -> u32 {
        0x00
    }

    fn write_to(&self, buffer: &mut Vec<u8>) {
        data::write_string(buffer, &serde_json::to_string(&self.reason).unwrap());
    }

    async fn read_from(
        connection: &mut PlayerConnection,
    ) -> Result<Box<LoginDisconnectPacket>, Error> {
        let (_, id, buffer) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        let mut position = 0 as usize;

        let j: Value =
            serde_json::from_str(&data::read_string(&buffer, &mut position).unwrap()).unwrap();

        Ok(Box::new(LoginDisconnectPacket {
            reason: FormattedText::deserialize(&j).unwrap(),
        }))
    }
}

impl PlayerboundPacket for LoginDisconnectPacket {}

#[derive(Clone)]
pub struct LoginStartPacket {
    pub username: String,
    pub uuid: Uuid,
}

impl Packet for LoginStartPacket {
    fn id() -> u32 {
        0x00
    }

    fn write_to(&self, buffer: &mut Vec<u8>) {
        data::write_string(buffer, &self.username);
        data::write_uuid(buffer, &self.uuid);
    }

    async fn read_from(connection: &mut PlayerConnection) -> Result<Box<LoginStartPacket>, Error> {
        let (_, id, buffer) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        let mut position = 0 as usize;

        Ok(Box::new(LoginStartPacket {
            username: data::read_string(&buffer, &mut position).unwrap(),
            uuid: data::read_uuid(&buffer, &mut position).unwrap(),
        }))
    }
}

impl PlayerboundPacket for LoginStartPacket {}
impl ProxyboundPacket for LoginStartPacket {}

impl LoginStartPacket {
    pub fn as_player_info(&self) -> PlayerInfo {
        PlayerInfo {
            username: self.username.to_owned(),
            uuid: self.uuid,
        }
    }
}

#[derive(Clone)]
pub struct LoginSuccessPacket {}

impl Packet for LoginSuccessPacket {
    fn id() -> u32 {
        0x02
    }

    fn write_to(&self, _: &mut Vec<u8>) {
        ()
    }

    async fn read_from(connection: &mut PlayerConnection) -> Result<Box<Self>, Error> {
        let (_, id, _) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        Ok(Box::new(LoginSuccessPacket {}))
    }
}

impl PlayerboundPacket for LoginSuccessPacket {}
