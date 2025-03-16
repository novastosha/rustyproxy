pub mod packet;
pub mod player;
pub mod server;

use std::{collections::HashMap, error::Error, sync::Arc};

use azalea_chat::{text_component::TextComponent, FormattedText};
use packet::{handshake::HandshakePacket, login::LoginDisconnectPacket, ProxyboundPacket};
use player::PlayerConnection;
use server::ProxiedServer;
use tokio::{net::TcpListener, task};

pub struct ProxyConfiguration {
    pub proxy_port: i16,

    pub address: Option<&'static String>,

    pub servers: Option<HashMap<String, ProxiedServer>>,
}

pub struct ProxyInstance {
    pub servers: HashMap<String, ProxiedServer>,
    pub config: ProxyConfiguration,
}

impl ProxyInstance {
    pub async fn start(self: Arc<Self>) -> Result<(), std::io::Error> {
        let address = self.config.address.map_or("0.0.0.0", |v| v); // Default to localhost
        let socket = TcpListener::bind(format!("{}:{}", address, self.config.proxy_port)).await?;

        loop {
            println!("Accepting connection...");
            match socket.accept().await {
                Ok((stream, addr)) => {
                    let instance = Arc::clone(&self); // Clone Arc to share across tasks

                    // Spawn a new task to handle the connection
                    task::spawn(async move {
                        let mut connection = PlayerConnection::new(stream, addr);

                        if instance.servers.is_empty() {
                            let handshake = HandshakePacket::read_from(&mut connection).await.ok();

                            if let Some(handshake) = handshake {
                                if handshake.next_state == 2 {
                                    let _ = connection.send_packet(&LoginDisconnectPacket {
                                        reason: FormattedText::Text(
                                            TextComponent::new("§cThere is no server to forward you to.\nrustyproxy - 0.10.0".to_owned())
                                        ),
                                    }).await;
                                }
                            }

                            let _ = connection.close().await; // Ensure connection is closed
                        }

                        // Additional connection handling...
                    });
                }
                Err(e) => eprintln!("Failed to accept connection: {:?}", e),
            }
        }
    }
}

pub fn new_instance(config: ProxyConfiguration) -> Result<ProxyInstance, Box<dyn Error>> {
    Ok(ProxyInstance {
        servers: config.servers.clone().unwrap_or_else(HashMap::new),
        config,
    })
}
