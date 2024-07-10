use structopt::StructOpt;

use crate::BlockHeight;

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "indexer", about = "Bitcoin Indexer")]
pub struct Config {
    #[structopt(
        short = "d",
        long = "database-url",
        env = "DATABASE_URL",
        hide_env_values = true
    )]
    pub database_url: String,

    #[structopt(
        short = "r",
        long = "node-rpc-url",
        env = "NODE_RPC_URL",
        hide_env_values = true
    )]
    pub node_rpc_url: String,

    #[structopt(short = "h", long = "height-to-sync", env = "HEIGHT_TO_SYNC")]
    pub height_to_sync: Option<BlockHeight>,

    #[structopt(short = "w", long = "wipe-whole-db")]
    pub wipe_db: bool,
}
