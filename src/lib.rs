pub mod event;
pub mod packet;
pub mod player;
pub mod server;

use std::{collections::HashMap, error::Error, fs, future::Future, pin::Pin, sync::Arc};

use azalea_chat::{text_component::TextComponent, FormattedText};
use event::{EventBus, EventResult, PlayerJoinedProxy, ProxyFinishedInitialization};
use packet::{
    handshake::HandshakePacket, login::{self, LoginDisconnectPacket, LoginStartPacket, LoginSuccessPacket}, play::SystemChatMessagePacket, Packet
};
use player::{PlayerConnection, PlayerInfo, TrafficForwardingResult};
use serde::Deserialize;
use server::ProxiedServer;
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
    task,
};

#[derive(Deserialize)]
pub struct ProxyConfiguration {
    pub proxy_port: i16,
    pub address: Option<String>,
    pub servers: Option<HashMap<String, ProxiedServer>>,
}

impl ProxyConfiguration {
    pub fn from_file(path: &str) -> Result<Self, std::io::Error> {
        let config_content = fs::read_to_string(path)?;
        let config: ProxyConfiguration = toml::from_str(&config_content).unwrap();
        Ok(config)
    }
}

pub struct ProxyInstance {
    pub servers: HashMap<String, Arc<ProxiedServer>>,
    pub config: ProxyConfiguration,
}

pub type SharedProxyInstance = Arc<tokio::sync::RwLock<ProxyInstance>>;

impl ProxyInstance {
    pub async fn start(
        instance: SharedProxyInstance,
        event_bus: Arc<EventBus>,
    ) -> Result<(), std::io::Error> {
        let address = {
            let instance = instance.read().await;
            instance
                .config
                .address
                .as_deref()
                .unwrap_or("0.0.0.0")
                .to_string()
        };

        let socket = TcpListener::bind(format!(
            "{}:{}",
            address,
            instance.read().await.config.proxy_port
        ))
        .await?;

        event_bus
            .dispatch(&Arc::new(ProxyFinishedInitialization))
            .await;

        loop {
            match socket.accept().await {
                Ok((stream, addr)) => {
                    let instance = Arc::clone(&instance);
                    let event_bus = Arc::clone(&event_bus);

                    task::spawn(async move {
                        let connection = Arc::new(Mutex::new(PlayerConnection::new(
                            stream, addr, &instance, &event_bus,
                        )));

                        let mut cnx = connection.lock().await;
                        let handshake = HandshakePacket::read_from(&mut cnx).await;

                        if let Err(_) = handshake {
                            let _ = cnx.close().await;
                            return;
                        }

                        let handshake = handshake.unwrap();
                        let proxy = instance.read().await;

                        if proxy.servers.is_empty() && handshake.next_state == 2 {
                            let _ = cnx
                                .send_packet(&LoginDisconnectPacket {
                                    reason: FormattedText::Text(TextComponent::new(
                                        "§cThere are currently no servers available.".to_owned(),
                                    )),
                                })
                                .await;
                            return;
                        }

                        drop(proxy);

                        if handshake.next_state == 2 {
                            let login_packet = LoginStartPacket::read_from(&mut cnx).await;
                            if let Err(_) = login_packet {
                                let _ = cnx.close().await;
                                return;
                            }

                            cnx.set_player_info(login_packet.unwrap().as_player_info())
                                .await;

                            drop(cnx);

                            let event = Arc::new(PlayerJoinedProxy {
                                connection: Arc::clone(&connection),
                            });

                            let result = event_bus.dispatch(&event).await;

                            if let Some(result) = result {
                                if result == EventResult::Stop {
                                    return;
                                }
                            }

                            fn recursively_connect(
                                connection: Arc<Mutex<PlayerConnection>>, 
                                instance: SharedProxyInstance,
                                retry: bool
                            ) -> Pin<Box<impl Future<Output =  ()>>> {
                                Box::pin(async move {
                                    let mut cnx = connection.lock().await;
                                    let server = {
                                        let instance = instance.read().await;
                            
                                        instance
                                            .servers
                                            .get(&"local_unauthenticated".to_owned())
                                            .unwrap()
                                            .clone()
                                    };
                            
                                    cnx.connect_to(&server).await.unwrap();

                                    if retry {
                                        cnx.compression_threshold = 256;
                                        let _ = cnx.send_packet_to_server(&LoginSuccessPacket {}).await;

                                        let _ = cnx.read_packet().await;

                                    }

                                    let result = cnx.handle_traffic().await;
                                    match result {
                                        TrafficForwardingResult::ServerDisconnectedPlayer()
                                        | TrafficForwardingResult::ServerErrored | TrafficForwardingResult::ServerKickedPlayer => {
                                            if result == TrafficForwardingResult::ServerKickedPlayer {
                                                let _ = cnx.send_packet(&SystemChatMessagePacket {
                                                    text: azalea_chat::FormattedText::Text(TextComponent::new(format!("§cYou were kicked from the server!").to_owned())),
                                                    overlay: false,
                                                }).await;
                                            }

                                            //let _ = cnx.server_closed().await;


                                            //recursively_connect(connection, instance,true).await;
                                        }
                            
                                        TrafficForwardingResult::PlayerDisconnected
                                        | TrafficForwardingResult::PlayerErrored => {
                                            let _ = cnx.close().await;
                                            println!("d");
                                        }
                                    }
                                })
                            }

                            recursively_connect(connection, instance,false).await
                        }
                    });
                }
                Err(e) => eprintln!("Failed to accept connection: {:?}", e),
            }
        }
    }
}

pub fn new_instance(config: ProxyConfiguration) -> Result<SharedProxyInstance, Box<dyn Error>> {
    Ok(Arc::new(RwLock::new(ProxyInstance {
        servers: config
            .servers
            .clone()
            .map(|y| {
                y.into_iter()
                    .map(|(key, value)| (key, Arc::new(value)))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_else(HashMap::new),
        config,
    })))
}
