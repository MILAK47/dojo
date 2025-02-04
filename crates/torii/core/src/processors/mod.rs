use anyhow::{Error, Result};
use async_trait::async_trait;
use dojo_world::contracts::world::WorldContractReader;
use starknet::core::types::{BlockWithTxs, Event, InvokeTransactionReceipt, TransactionReceipt};
use starknet::providers::Provider;

use crate::sql::Sql;

pub mod metadata_update;
pub mod register_model;
pub mod store_set_record;

#[async_trait]
pub trait EventProcessor<P>
where
    P: Provider,
{
    fn event_key(&self) -> String;

    #[allow(clippy::too_many_arguments)]
    async fn process(
        &self,
        world: &WorldContractReader<P>,
        db: &mut Sql,
        block: &BlockWithTxs,
        invoke_receipt: &InvokeTransactionReceipt,
        event_id: &str,
        event: &Event,
    ) -> Result<(), Error>;
}

#[async_trait]
pub trait BlockProcessor<P: Provider + Sync> {
    fn get_block_number(&self) -> String;
    async fn process(&self, db: &mut Sql, provider: &P, block: &BlockWithTxs) -> Result<(), Error>;
}

#[async_trait]
pub trait TransactionProcessor<P: Provider + Sync> {
    async fn process(
        &self,
        db: &mut Sql,
        provider: &P,
        block: &BlockWithTxs,
        transaction_receipt: &TransactionReceipt,
    ) -> Result<(), Error>;
}
