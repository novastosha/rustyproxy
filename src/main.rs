use std::{collections::HashMap, sync::Arc};

use rustyproxy::{server::ProxiedServer, ProxyConfiguration};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let mut servers = HashMap::new();

    servers.insert("hypixel".to_owned(), ProxiedServer {
        address: std::net::IpAddr::V4("mc.hypixel.net".parse().unwrap())
    });
    
    let instance = Arc::new(
        rustyproxy::new_instance(ProxyConfiguration {
            proxy_port: 25565,
            address: None,
            servers: None
        }).unwrap()
    );

    

    instance.start().await
}