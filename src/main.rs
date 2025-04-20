use std::{collections::HashMap, sync::Arc};

use azalea_chat::text_component::TextComponent;
use rustyproxy::{
    event::{EventBus, EventResult, PlayerJoinedProxy, PlayerJoinedServer, ProxyFinishedInitialization, ServerSentPacket}, packet::{self, configuration::{channels::BrandChannel, PlayerConfigurationPluginMessagePacket}, login::LoginDisconnectPacket}, player::{ConnectionState, PlayerInfo}, server::ProxiedServer, ProxyConfiguration, ProxyInstance
};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let instance = rustyproxy::new_instance(
        ProxyConfiguration::from_file("example/config.toml").unwrap()
    )
    .unwrap();

    let event_bus = EventBus::new(&instance);

    event_bus
        .listen::<PlayerJoinedProxy, _, _, _>(false, |instance, _| async move {
            let instance = instance.read().await;

            let cloned_servers: HashMap<String, ProxiedServer> = instance
                .servers
                .iter()
                .map(|(key, value)| (key.clone(), value.as_ref().clone())) // Clone key & inner ProxiedServer
                .collect();

            let serialized_data: String = serde_json::to_string(&cloned_servers).unwrap();
            println!("{}", serialized_data);

            None
        })
        .await;

    event_bus
        .listen::<ProxyFinishedInitialization, _, _, _>(true, |instance, _| async move {
            let mut instance = instance.write().await;

            instance.servers.insert(
                "local".to_string(),
                Arc::new(ProxiedServer::new("Localhost".to_string(),"0.0.0.0".to_owned(), 25565)),
            );
            println!("finished");

            None
        })
        .await;

    event_bus
        .listen::<PlayerJoinedServer, _, _, _>(false, |_, event| async move {
            let mut player = event.connection.lock().await;
            
            let player_info = {
                let player_info = player.player_info.lock().await;
                player_info.clone().unwrap()
            };       
            player.set_player_info(PlayerInfo {
                uuid: player_info.uuid,
                username: format!("rustyproxy_{}",player_info.username)
            }).await;

            

            None
        })
        .await;

    event_bus.listen::<ServerSentPacket, _,_,_>(false, |_, event| async move {
        let mut connection = event.connection.lock().await;
        let server_guard = connection.server.lock().await;
        let server = server_guard.as_ref().unwrap();


        
        let (_, id, data) = &event.packet;
        if *id != 0x01 || server.state != Some(ConnectionState::Configuration) {
            return None
        }

        drop(server_guard);

        let mut position = 0 as usize;
        let channel_name = packet::data::read_string(&data, &mut position).unwrap();

        if channel_name != "minecraft:brand" {
            return None
        }

        let brand = packet::data::read_string(&data, &mut position).unwrap();
        let channel_data = BrandChannel { brand: format!("{} (rustyproxy)", brand) };

        let constructed_packet = PlayerConfigurationPluginMessagePacket { data: channel_data };

        let _ = connection.send_packet(&constructed_packet).await;
        Some(EventResult::Stop)
    }).await;

    ProxyInstance::start(instance, event_bus).await
}
