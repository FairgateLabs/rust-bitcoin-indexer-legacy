use bitcoin_indexer::{db::pg::IndexerStore, indexer::Indexer, opts::Config};
use common_failures::{prelude::*, quick_main};
use structopt::StructOpt;

fn run() -> Result<()> {
    dotenv::dotenv()?;
    env_logger::init();

    //TODO: [Future improvement] - We can start using clap v3 instead structOpt
    let opts: Config = StructOpt::from_args();

    if opts.wipe_db {
        IndexerStore::wipe(&opts.database_url)?;
        return Ok(());
    }

    let mut indexer = Indexer::new(opts)?;
    indexer.run()?;

    Ok(())
}

quick_main!(run);
