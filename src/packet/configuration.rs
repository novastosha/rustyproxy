use std::io::{Error, ErrorKind};

use crate::{player::PlayerConnection, server::plugin_channel::PluginChannel};

use super::{data, handshake::HandshakePacket, Packet, PlayerboundPacket, ProxyboundPacket};

#[derive(Clone)]
pub struct PlayerConfigurationPluginMessagePacket<T: PluginChannel> {
    pub data: T,
}

impl<T: PluginChannel> Packet for PlayerConfigurationPluginMessagePacket<T> {
    fn id() -> u32 {
        0x01
    }
    async fn read_from(
        connection: &mut PlayerConnection,
    ) -> Result<Box<PlayerConfigurationPluginMessagePacket<T>>, Error> {
        let (_, id, buffer) = connection.read_packet().await?;
        if id != Self::id() {
            return Err(Error::new(ErrorKind::Other, "id mismatch!"));
        }

        let mut position = 0 as usize;
        let channel_name = data::read_string(&buffer, &mut position).unwrap();

        if channel_name != T::id() {
            return Err(Error::new(ErrorKind::Other, "channel id mismatch!"));
        }

        Ok(Box::new(PlayerConfigurationPluginMessagePacket {
            data: *T::read_from(&mut buffer[position..].to_vec())
                .await
                .unwrap(),
        }))
    }

    fn write_to(&self, buffer: &mut Vec<u8>) {
        data::write_string(buffer, &T::id());
        self.data.write_to(buffer);
    }
}

impl<T: PluginChannel> PlayerboundPacket for PlayerConfigurationPluginMessagePacket<T> {}

pub mod channels {
    use crate::{packet::data, server::plugin_channel::PluginChannel};

    #[derive(Clone)]
    pub struct BrandChannel {
        pub brand: String,
    }

    impl PluginChannel for BrandChannel {
        fn id() -> String {
            "minecraft:brand".to_owned()
        }

        fn write_to(&self, buffer: &mut Vec<u8>) {
            data::write_string(buffer, &self.brand);
        }

        async fn read_from(buffer: &mut Vec<u8>) -> Result<Box<Self>, std::io::Error> {
            Ok(
                Box::new(
                    BrandChannel { brand: data::read_string(buffer, &mut 0).unwrap() }
                )
            )            
        }
    }
}
