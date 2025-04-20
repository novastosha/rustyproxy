use std::{
    io::{Error, ErrorKind},
    net::SocketAddr,
    sync::Arc,
};

use azalea_chat::text_component::TextComponent;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

use crate::{
    event::{EventBus, EventResult, PlayerJoinedServer, ServerSentPacket},
    packet::{
        self, data, handshake::HandshakePacket, login::LoginStartPacket,
        play::SystemChatMessagePacket, PlayerboundPacket, RawPacket,
    },
    server::ProxiedServer,
    ProxyInstance, SharedProxyInstance,
};

#[derive(Clone)]
pub struct PlayerConnection {
    pub proxy_instance: Arc<RwLock<ProxyInstance>>,
    pub addr: SocketAddr,

    cnx: Arc<Mutex<TcpStream>>,
    pub server: Arc<Mutex<Option<PlayerProxyConnection>>>,

    pub player_info: Arc<Mutex<Option<PlayerInfo>>>,
    pub compression_threshold: u32,
    event_bus: Arc<EventBus>,
}

#[derive(Clone)]
pub struct PlayerInfo {
    pub username: String,
    pub uuid: Uuid,
}

pub struct PlayerProxyConnection {
    cnx: TcpStream,
    player: Arc<Mutex<PlayerConnection>>,
    server: Arc<ProxiedServer>,
    pub state: Option<ConnectionState>,
}

#[derive(PartialEq, Eq)]
pub enum ConnectionResult {
    Success,
    Disconnected,
    Error,
}

#[derive(PartialEq, Eq)]
pub enum TrafficForwardingResult {
    ServerDisconnectedPlayer(),
    PlayerDisconnected,
    ServerErrored,
    PlayerErrored,
    ServerKickedPlayer,
}

#[derive(PartialEq, Eq, Clone)]
pub enum ConnectionState {
    Handshake,
    Login,
    Configuration,
    Play,
}

impl PlayerConnection {
    pub fn new(
        cnx: TcpStream,
        addr: SocketAddr,
        proxy_instance: &SharedProxyInstance,
        event_bus: &Arc<EventBus>,
    ) -> PlayerConnection {
        PlayerConnection {
            compression_threshold: 0,
            cnx: Arc::new(Mutex::new(cnx)),
            addr,
            server: Arc::new(Mutex::const_new(None)),
            player_info: Arc::new(Mutex::const_new(None)),
            proxy_instance: Arc::clone(proxy_instance),
            event_bus: event_bus.clone(),
        }
    }

    pub async fn set_player_info(&mut self, info: PlayerInfo) {
        let mut info_guard = self.player_info.lock().await;
        *info_guard = Some(info)
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.cnx.lock().await.shutdown().await?;

        let mut proxy_cnx = self.server.lock().await;

        if let Some(server) = proxy_cnx.as_mut() {
            server.close().await?
        }

        *proxy_cnx = None;
        Ok(())
    }

    pub(crate) async fn server_closed(&mut self) -> Result<(), Error> {
        let old_server: Arc<ProxiedServer>;
        {
            let mut proxy_cnx = self.server.lock().await;

            if let Some(server) = proxy_cnx.as_mut() {
                server.close().await?;
            }

            old_server = proxy_cnx.as_ref().unwrap().server.clone();
            *proxy_cnx = None;
        }

        Ok(())
    }

    pub async fn connect_to(
        &mut self,
        server: &Arc<ProxiedServer>,
    ) -> Result<ConnectionResult, Error> {
        {
            let mut server = self.server.lock().await;
            if let Some(proxy_cnx) = server.as_mut() {
                proxy_cnx.close().await?;
            }
        }

        let cloned_player = Arc::new(Mutex::new(self.clone()));

        let event = Arc::new(PlayerJoinedServer {
            connection: cloned_player.clone(),
            server: server.clone(),
        });

        let proceed = self.event_bus.dispatch(&event).await;

        if let Some(result) = proceed {
            if result == EventResult::Stop {
                return Err(Error::new(ErrorKind::ConnectionAborted, "Cancelled"));
            }
        }

        let mut connection = PlayerProxyConnection {
            cnx: server.establish_connection().await?,
            player: cloned_player.clone(),
            server: Arc::clone(server),
            state: Some(ConnectionState::Login),
        };

        self.compression_threshold = 0;

        packet::send_packet(
            &HandshakePacket {
                protocol: 769,
                server_address: server.address.clone(),
                port: server.port,
                next_state: 2,
            },
            &mut connection.cnx,
            self.compression_threshold,
        )
        .await?; // Send a handshake as soon as we establish a connection

        {
            let info = self.player_info.lock().await;
            let info = info.as_ref().unwrap();

            packet::send_packet(
                &LoginStartPacket {
                    username: info.username.to_owned(),
                    uuid: info.uuid,
                },
                &mut connection.cnx,
                self.compression_threshold,
            )
            .await?;
        }

        {
            let mut server = self.server.lock().await;
            *server = Some(connection);
        }

        Ok(ConnectionResult::Success)
    }

    pub async fn is_connected(&self) -> bool {
        let server = self.server.lock().await;
        match *server {
            None => false,
            Some(_) => true,
        }
    }

    pub(crate) async fn handle_traffic(&mut self) -> TrafficForwardingResult {
        let mut server_guard = self.server.lock().await;
        match server_guard.as_mut() {
            None => return TrafficForwardingResult::PlayerDisconnected,
            Some(_) => (),
        };

        drop(server_guard);

        let mut buffer = vec![0u8; 4096*12];

        let mut buffer_recv = vec![0u8; 4096*12];

        loop {
            tokio::select! {
                result = async {
                    let stream = &mut self.cnx;

                    let mut stream = stream.lock().await;
                    stream.read(&mut buffer).await
                } => {
                    match result {
                        Ok(0) => return TrafficForwardingResult::PlayerDisconnected, // Client disconnected
                        Ok(n) => {

                            let mut server_guard = self.server.lock().await;
                            let proxy_connection =  &mut server_guard.as_mut().unwrap().cnx;
                            if let Err(e) = proxy_connection.write_all(&buffer[..n]).await {
                                if e.kind() == std::io::ErrorKind::BrokenPipe {
                                    return TrafficForwardingResult::PlayerErrored;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading from player: {:?}", e);
                            return TrafficForwardingResult::PlayerErrored;
                        }
                    }
                }

                result = async {
                    let mut server_guard = self.server.lock().await;
                    let proxy_connection =  &mut server_guard.as_mut().unwrap().cnx;
                    proxy_connection.read(&mut buffer_recv).await
                } => {
                    match result {
                        Ok(0) => return TrafficForwardingResult::ServerDisconnectedPlayer(), // Proxy server disconnected
                        Ok(n) => {
                            let mut server_guard = self.server.lock().await;
                            let server = &mut server_guard.as_mut().unwrap();

                            let state = {
                                server.state.clone().unwrap_or_else(|| ConnectionState::Handshake)
                            };

                            let (length, id, data) = packet::read_packet_from_bytes(&buffer_recv, self.compression_threshold).unwrap();
                            if id == 0x03 && state == ConnectionState::Login { // Login Compression
                                self.compression_threshold = data::read_varint(&data, &mut 0).unwrap();
                            } // Change this so the proxy is the one to set the compression threshold.

                            if id == 0x02 && state == ConnectionState::Login {
                                server.state = Some(ConnectionState::Configuration);
                            }

                            if id == 0x02 && state == ConnectionState::Configuration {
                                server.state = Some(ConnectionState::Play)
                            }

                            if id == 0x1D {
                                return TrafficForwardingResult::ServerKickedPlayer;
                            }

                            drop(server_guard);

                            {
                                let cloned_player = Arc::new(Mutex::new(self.clone()));

                                let event = Arc::new(ServerSentPacket {connection:cloned_player, packet: (length,id,data.clone()) });
                                let proceed = self.event_bus.dispatch(&event).await;

                                if proceed == Some(EventResult::Stop) {
                                    println!("Skipped sending a packet");
                                    continue; // Skip sending this packet.
                                }
                            }

                            let stream = &mut self.cnx;
                            let mut stream = stream.lock().await;
                            if let Err(e) = stream.write_all(&buffer_recv[..n]).await {
                                if e.kind() == std::io::ErrorKind::BrokenPipe {
                                    return TrafficForwardingResult::ServerDisconnectedPlayer();
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading from proxied server: {:?}", e);
                            return TrafficForwardingResult::ServerErrored;
                        }
                    }
                }
            }
        }
    }

    pub async fn read_packet(&mut self) -> Result<RawPacket, Error> {
        let mut locked_connection = self.cnx.lock().await;
        packet::read_packet(&mut locked_connection, self.compression_threshold).await
    }

    pub async fn send_packet<P: PlayerboundPacket>(&mut self, packet: &P) -> Result<(), Error> {
        let mut locked_connection = self.cnx.lock().await;
        packet::send_packet(packet, &mut locked_connection, self.compression_threshold).await
    }

    pub async fn send_packet_to_server<P: PlayerboundPacket>(
        &mut self,
        packet: &P,
    ) -> Result<(), Error> {
        let mut server_connection = self.server.lock().await;

        let mut locked_connection = &mut server_connection.as_mut().unwrap().cnx;
        packet::send_packet(packet, &mut locked_connection, self.compression_threshold).await
    }
}

impl PlayerProxyConnection {
    pub async fn close(&mut self) -> Result<(), Error> {
        self.cnx.shutdown().await?;
        Ok(())
    }
}
