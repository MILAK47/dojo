use std::str::FromStr;

use anyhow::Result;
use async_graphql::dynamic::Schema;
use dojo_test_utils::compiler::build_test_config;
use dojo_test_utils::migration::prepare_migration;
use dojo_test_utils::sequencer::{
    get_default_test_starknet_config, SequencerConfig, TestSequencer,
};
use dojo_types::primitive::Primitive;
use dojo_types::schema::{Enum, EnumOption, Member, Struct, Ty};
use dojo_world::contracts::WorldContractReader;
use dojo_world::utils::TransactionWaiter;
use scarb::ops;
use serde::Deserialize;
use serde_json::Value;
use sozo::ops::migration::execute_strategy;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use starknet::accounts::{Account, Call};
use starknet::core::types::{BlockId, BlockTag, FieldElement, InvokeTransactionResult};
use starknet::macros::selector;
use starknet::providers::jsonrpc::HttpTransport;
use starknet::providers::JsonRpcClient;
use tokio_stream::StreamExt;
use torii_core::engine::{Engine, EngineConfig, Processors};
use torii_core::processors::register_model::RegisterModelProcessor;
use torii_core::processors::store_set_record::StoreSetRecordProcessor;
use torii_core::sql::Sql;

mod entities_test;
mod models_test;
mod subscription_test;

use crate::schema::build_schema;

#[derive(Deserialize, Debug, PartialEq)]
pub struct Connection<T> {
    pub total_count: i64,
    pub edges: Vec<Edge<T>>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Edge<T> {
    pub node: T,
    pub cursor: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Entity {
    pub model_names: String,
    pub keys: Option<Vec<String>>,
    pub created_at: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Moves {
    pub __typename: String,
    pub remaining: u32,
    pub last_direction: String,
    pub entity: Option<Entity>,
}

#[derive(Deserialize, Debug)]
pub struct Vec2 {
    pub x: u32,
    pub y: u32,
}

#[derive(Deserialize, Debug)]
pub struct Position {
    pub __typename: String,
    pub vec: Vec2,
    pub entity: Option<Entity>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Record {
    pub __typename: String,
    pub record_id: u32,
    pub type_u8: u8,
    pub type_u16: u16,
    pub type_u32: u32,
    pub type_u64: u64,
    pub type_u128: String,
    pub type_u256: String,
    pub type_bool: bool,
    pub type_felt: String,
    pub type_class_hash: String,
    pub type_contract_address: String,
    pub random_u8: u8,
    pub random_u128: String,
    pub type_nested: Option<Nested>,
    pub entity: Option<Entity>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Nested {
    pub __typename: String,
    pub depth: u8,
    pub type_number: u8,
    pub type_string: String,
    pub type_nested_more: NestedMore,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct NestedMore {
    pub __typename: String,
    pub depth: u8,
    pub type_number: u8,
    pub type_string: String,
    pub type_nested_more_more: NestedMoreMore,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct NestedMoreMore {
    pub __typename: String,
    pub depth: u8,
    pub type_number: u8,
    pub type_string: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Subrecord {
    pub __typename: String,
    pub record_id: u32,
    pub subrecord_id: u32,
    pub type_u8: u8,
    pub random_u8: u8,
    pub entity: Option<Entity>,
}

pub async fn run_graphql_query(schema: &Schema, query: &str) -> Value {
    let res = schema.execute(query).await;

    assert!(res.errors.is_empty(), "GraphQL query returned errors: {:?}", res.errors);
    serde_json::to_value(res.data).expect("Failed to serialize GraphQL response")
}

#[allow(dead_code)]
pub async fn run_graphql_subscription(
    pool: &SqlitePool,
    subscription: &str,
) -> async_graphql::Value {
    // Build dynamic schema
    let schema = build_schema(pool).await.unwrap();
    schema.execute_stream(subscription).next().await.unwrap().into_result().unwrap().data
    // fn subscribe() is called from inside dynamic subscription
}

pub async fn model_fixtures(db: &mut Sql) {
    db.register_model(
        Ty::Struct(Struct {
            name: "Moves".to_string(),
            children: vec![
                Member {
                    name: "player".to_string(),
                    key: true,
                    ty: Ty::Primitive(Primitive::ContractAddress(None)),
                },
                Member {
                    name: "remaining".to_string(),
                    key: false,
                    ty: Ty::Primitive(Primitive::U8(None)),
                },
                Member {
                    name: "last_direction".to_string(),
                    key: false,
                    ty: Ty::Enum(Enum {
                        name: "Direction".to_string(),
                        option: None,
                        options: vec![
                            EnumOption { name: "None".to_string(), ty: Ty::Tuple(vec![]) },
                            EnumOption { name: "Left".to_string(), ty: Ty::Tuple(vec![]) },
                            EnumOption { name: "Right".to_string(), ty: Ty::Tuple(vec![]) },
                            EnumOption { name: "Up".to_string(), ty: Ty::Tuple(vec![]) },
                            EnumOption { name: "Down".to_string(), ty: Ty::Tuple(vec![]) },
                        ],
                    }),
                },
            ],
        }),
        vec![],
        FieldElement::ONE,
        0,
        0,
    )
    .await
    .unwrap();

    db.register_model(
        Ty::Struct(Struct {
            name: "Position".to_string(),
            children: vec![
                Member {
                    name: "player".to_string(),
                    key: true,
                    ty: Ty::Primitive(Primitive::ContractAddress(None)),
                },
                Member {
                    name: "vec".to_string(),
                    key: false,
                    ty: Ty::Struct(Struct {
                        name: "Vec2".to_string(),
                        children: vec![
                            Member {
                                name: "x".to_string(),
                                key: false,
                                ty: Ty::Primitive(Primitive::U32(None)),
                            },
                            Member {
                                name: "y".to_string(),
                                key: false,
                                ty: Ty::Primitive(Primitive::U32(None)),
                            },
                        ],
                    }),
                },
            ],
        }),
        vec![],
        FieldElement::TWO,
        0,
        0,
    )
    .await
    .unwrap();
}

pub async fn spinup_types_test() -> Result<SqlitePool> {
    // change sqlite::memory: to sqlite:~/.test.db to dump database to disk
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    let migration = prepare_migration("./src/tests/types-test/target/dev".into()).unwrap();
    let config = build_test_config("./src/tests/types-test/Scarb.toml").unwrap();
    let mut db = Sql::new(pool.clone(), migration.world_address().unwrap()).await.unwrap();

    let sequencer =
        TestSequencer::start(SequencerConfig::default(), get_default_test_starknet_config()).await;

    let mut account = sequencer.account();
    account.set_block_id(BlockId::Tag(BlockTag::Pending));

    let provider = JsonRpcClient::new(HttpTransport::new(sequencer.url()));
    let world = WorldContractReader::new(migration.world_address().unwrap(), &provider);
    let ws = ops::read_workspace(config.manifest_path(), &config)
        .unwrap_or_else(|op| panic!("Error building workspace: {op:?}"));

    execute_strategy(&ws, &migration, &account, None).await.unwrap();

    //  Execute `create` and insert 10 records into storage
    let records_contract = "0x4ff40a178c593ce3cb432b020b8546508f27048a56e1256694b459ba78de001";
    let InvokeTransactionResult { transaction_hash } = account
        .execute(vec![Call {
            calldata: vec![FieldElement::from_str("0xa").unwrap()],
            to: FieldElement::from_str(records_contract).unwrap(),
            selector: selector!("create"),
        }])
        .send()
        .await
        .unwrap();

    TransactionWaiter::new(transaction_hash, &provider).await?;

    let mut engine = Engine::new(
        world,
        &mut db,
        &provider,
        Processors {
            event: vec![Box::new(RegisterModelProcessor), Box::new(StoreSetRecordProcessor)],
            ..Processors::default()
        },
        EngineConfig::default(),
        None,
    );

    let _ = engine.sync_to_head(0).await?;

    Ok(pool)
}
