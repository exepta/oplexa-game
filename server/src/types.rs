use multiplayer::world::NetworkEntity;
use naia_server::Server as NaiaServer;

pub type Server = NaiaServer<NetworkEntity>;
