use std::{io::Error, net::SocketAddr, sync::Arc};

use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};

use crate::{packet::{data, PlayerboundPacket}, server::ProxiedServer};

pub struct PlayerConnection {
    cnx: TcpStream,
    pub addr: SocketAddr,

    pub server: Option<Arc<ProxiedServer>>
}

impl PlayerConnection {
    pub fn new(cnx: TcpStream, addr: SocketAddr) -> PlayerConnection {
        PlayerConnection { cnx, addr, server: None }
    }
    
    pub async fn close(mut self) -> Result<(), Error> {
        self.cnx.shutdown().await?;
        Ok(())
    }
    
    pub async fn read_packet_raw(
        &mut self,
    ) -> Result<(u32, u32, Vec<u8>), Error> {
        let stream = &mut self.cnx;

        let mut temp_buffer = vec![0u8; 5];
        stream.read_exact(&mut temp_buffer[..1]).await?;
    
        let mut position = 0;
        let mut slice = &temp_buffer[..];
        let packet_length = data::read_varint(&mut slice, &mut position)?;
    
        let mut buffer = vec![0u8; packet_length as usize];
        stream.read_exact(&mut buffer).await?;
    
        let mut position = 0; 
        let mut slice = &buffer[..];
        let packet_id = data::read_varint(&mut slice, &mut position)?;
    
        let data_start = position; 
        let data = buffer[data_start..].to_vec();
    
        Ok((packet_length, packet_id, data))
    }
    
    pub async fn send_packet<P: PlayerboundPacket>(&mut self, packet: &P) -> Result<(), Error> {
    
        let mut buffer = Vec::new();

        let packet_id = P::id();
        data::write_varint(&mut buffer, packet_id);

        let mut data_buffer = Vec::new();
        packet.write_to(&mut data_buffer);
        buffer.extend(data_buffer);

        let total_length = buffer.len();

        let mut final_buffer = Vec::new();
        data::write_varint(&mut final_buffer,total_length as u32);

        final_buffer.extend(buffer);

        self.cnx.write_all(&final_buffer).await?;
        self.cnx.flush().await?;

        Ok(())
    }
    
}