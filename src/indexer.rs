use bitcoincore_rpc::{Client, RpcApi};
use log::{error, info, warn};
use std::sync::Arc;

use crate::{
    db::{self, pg::IndexerStore},
    node::prefetcher::Prefetcher,
    opts::Config,
    util::{bitcoin::network_from_str, BottleCheck},
    BlockData, BlockHeight, RpcInfo,
};
use common_failures::prelude::*;

pub struct Indexer {
    height_to_sync: Option<BlockHeight>,
    node_starting_chainhead_height: BlockHeight,
    rpc: Arc<Client>,
    db: Box<dyn db::IndexerStore>,
    bottlecheck_db: BottleCheck,
}

impl Indexer {
    pub fn new(config: Config) -> Result<Self> {
        let rpc_info = RpcInfo::from_url(&config.node_rpc_url)?;
        let rpc = rpc_info.to_rpc_client()?;
        let rpc = Arc::new(rpc);
        let node_starting_chainhead_height = rpc.get_block_count()? as BlockHeight;
        let network = network_from_str(&rpc.get_blockchain_info()?.chain)?;
        let db = IndexerStore::new(config.database_url, node_starting_chainhead_height, network)?;
        info!("Node chain-head at {}H", node_starting_chainhead_height);

        Ok(Self {
            height_to_sync: config.height_to_sync,
            rpc,
            node_starting_chainhead_height,
            db: Box::new(db),
            bottlecheck_db: BottleCheck::new("database".into()),
        })
    }

    pub fn process_block(&mut self, block: BlockData, is_checkpoint: bool) -> Result<()> {
        let block_height = block.height;
        if block_height >= self.node_starting_chainhead_height || block_height % 1000 == 0 {
            eprintln!("Block {}H: {}", block.height, block.id);
        }

        let Self {
            ref mut db,
            ref mut bottlecheck_db,
            ..
        } = self;

        bottlecheck_db.check(|| db.insert(block, is_checkpoint))?;
        Ok(())
    }

    pub fn get_height_to_sync(&mut self) -> (u32, bool) {
        // node_starting_chainhead_height: The current block height of the Bitcoin network.
        // height_to_sync: The starting block height for synchronization.
        // last_indexed_height: The highest block height that has already been synchronized and stored in the database.

        let last_indexed_height = self.db.get_head_height().unwrap();

        if last_indexed_height.is_some() {
            info!("Last indexed block is {:?}H", last_indexed_height.unwrap());
        } else {
            info!("No block indexed");
        }

        let last_indexed_height = last_indexed_height.unwrap_or(0);

        let start_to_sync_from_height = match self.height_to_sync {
            Some(height_to_sync) => {
                if height_to_sync < last_indexed_height {
                    warn!("Passed HEIGHT_TO_SYNC command line is behind last indexed height");
                    info!(
                        "Using last indexed height {} instead HEIGHT_TO_SYNC {} to start to sync",
                        last_indexed_height, height_to_sync
                    );
                    (last_indexed_height, false)
                } else {
                    info!("Using HEIGHT_TO_SYNC={} to start to sync", height_to_sync);
                    (height_to_sync, true)
                }
            }
            None => (last_indexed_height, false),
        };

        // 3) ERROR: node_starting_chainhead_height < start_height
        if self.node_starting_chainhead_height < start_to_sync_from_height.0 {
            error!(
                "The current block height of the Bitcoin network is behind the starting block to sync"
            );
            panic!();
        }

        start_to_sync_from_height
    }

    pub fn run(&mut self) -> Result<()> {
        let start_to_sync = self.get_height_to_sync();

        let mut checkpoint: Option<u32> = None;

        if start_to_sync.1 {
            checkpoint = Some(start_to_sync.0);
        }

        let prefetcher = Prefetcher::new(self.rpc.clone(), start_to_sync.0, checkpoint)?;

        let mut bottlecheck_fetcher = BottleCheck::new("block fetcher".into());

        for block in bottlecheck_fetcher.check_iter(prefetcher) {
            let is_checkpoint = start_to_sync.1 && block.height == start_to_sync.0;
            self.process_block(block, is_checkpoint)?;
        }

        Ok(())
    }
}
