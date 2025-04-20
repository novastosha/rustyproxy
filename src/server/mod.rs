use std::io::Error;

use serde::Deserialize;
use tokio::net::TcpStream;



#[derive(Clone, Deserialize,serde::Serialize)]
pub struct ProxiedServer {
    pub address: String,
    pub port: u16,
    pub name: String
}
impl ProxiedServer {
    pub(crate) async  fn establish_connection(&self) -> Result<TcpStream, Error> {
        TcpStream::connect((self.address.as_str(), self.port)).await
    }

    pub fn new(name: String, address: String, port: u16) -> ProxiedServer {
        ProxiedServer {
            address,
            port,
            name,
        }
    }
}

pub mod plugin_channel {
    use std::io::Error;

    pub trait PluginChannel: Clone {
        fn id() -> String;
        fn write_to(&self, buffer: &mut Vec<u8>);
        fn read_from(
            buffer: &mut Vec<u8>,
        ) -> impl std::future::Future<Output = Result<Box<Self>, Error>> + Send;
        }
}