#![allow(unused)] // need some cleanup

use bitcoin_indexer::{
    db,
    node::prefetcher,
    opts::{self, Config},
    prelude::*,
    types::*,
    util::BottleCheck,
    RpcInfo,
};
use bitcoincore_rpc::RpcApi;
use log::info;
use std::{env, sync::Arc};
use structopt::StructOpt;

use common_failures::{prelude::*, quick_main};

struct Indexer {
    node_starting_chainhead_height: BlockHeight,
    rpc: Arc<bitcoincore_rpc::Client>,
    db: Box<dyn db::IndexerStore>,
    bottlecheck_db: BottleCheck,
}

impl Indexer {
    fn new(config: Config) -> Result<Self> {
        let rpc_info = bitcoin_indexer::RpcInfo::from_url(&config.node_rpc_url)?;
        let rpc = rpc_info.to_rpc_client()?;
        let rpc = Arc::new(rpc);
        let node_starting_chainhead_height = rpc.get_block_count()? as BlockHeight;
        let network =
            bitcoin_indexer::util::bitcoin::network_from_str(&rpc.get_blockchain_info()?.chain)?;
        let mut db = db::pg::IndexerStore::new(
            config.database_url,
            node_starting_chainhead_height,
            network,
        )?;
        info!("Node chain-head at {}H", node_starting_chainhead_height);

        Ok(Self {
            rpc,
            node_starting_chainhead_height,
            db: Box::new(db),
            bottlecheck_db: BottleCheck::new("database".into()),
        })
    }

    fn process_block(&mut self, block: BlockData) -> Result<()> {
        let block_height = block.height;
        if block_height >= self.node_starting_chainhead_height || block_height % 1000 == 0 {
            eprintln!("Block {}H: {}", block.height, block.id);
        }

        let Self {
            ref mut db,
            ref mut bottlecheck_db,
            ..
        } = self;

        bottlecheck_db.check(|| db.insert(block))?;
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        let start: Option<WithHeightAndId<BlockHash, _>> =
            if let Some(last_indexed_height) = self.db.get_head_height()? {
                info!("Last indexed block {}H", last_indexed_height);

                assert!(last_indexed_height <= self.node_starting_chainhead_height);
                let start_from_block = last_indexed_height.saturating_sub(100); // redo 100 last blocks, in case there was a reorg
                Some(WithHeightAndId {
                    height: start_from_block,
                    id: self
                        .db
                        .get_hash_by_height(start_from_block)?
                        .expect("Block hash should be there"),
                    data: (),
                })
            } else {
                None
            };

        let prefetcher = prefetcher::Prefetcher::new(self.rpc.clone(), start)?;
        let mut bottlecheck_fetcher = BottleCheck::new("block fetcher".into());
        for item in bottlecheck_fetcher.check_iter(prefetcher) {
            self.process_block(item)?;
        }

        Ok(())
    }
}

fn run() -> Result<()> {
    dotenv::dotenv()?;
    env_logger::init();

    //TODO: [Improvement] -  Consider switching from StructOpt to Clap v3 as it is a newer library.
    let opts: Config = StructOpt::from_args();

    if opts.wipe_db {
        db::pg::IndexerStore::wipe(&opts.database_url)?;
        return Ok(());
    }

    let mut indexer = Indexer::new(opts)?;
    indexer.run()?;

    Ok(())
}

quick_main!(run);
