use azalea_chat:: FormattedText;
use serde_json;

use super::{data, Packet, PlayerboundPacket};

#[derive(Clone)]
pub struct LoginDisconnectPacket {
    pub reason: FormattedText // Perhaps there is a library for text components
}

impl Packet for LoginDisconnectPacket {
    fn id() -> u32 {
        0x00
    }
}

impl PlayerboundPacket for LoginDisconnectPacket {
    fn write_to(&self, buffer: &mut Vec<u8>) {

        data::write_string(buffer, &serde_json::to_string(&self.reason).unwrap());
    }
}