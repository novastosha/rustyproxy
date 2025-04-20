use std::io::{Error, ErrorKind};

use azalea_chat::FormattedText;

use super::{data, Packet, PlayerboundPacket};

#[derive(Clone)]
pub struct SystemChatMessagePacket {
    pub text: FormattedText,
    pub overlay: bool
}

impl Packet for SystemChatMessagePacket {
    fn id() -> u32 {
        0x73
    }

    fn write_to(&self, buffer: &mut Vec<u8>) {
        data::nbt::write_text(buffer, &self.text);
        data::write_bool(buffer, self.overlay);
    }

    async fn read_from(
        connection: &mut crate::player::PlayerConnection,
    ) -> Result<Box<Self>, Error> {
        let (_, id, buffer) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        let mut position = 0 as usize;

        Ok(Box::new(SystemChatMessagePacket {
            text: data::nbt::read_text(&buffer, &mut position).unwrap(),
            overlay: data::read_bool(&buffer, &mut position).unwrap(),
        }))
    }
}
impl PlayerboundPacket for SystemChatMessagePacket {}
