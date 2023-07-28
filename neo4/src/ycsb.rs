use std::{collections::BTreeMap, iter::repeat_with};

use bincode::Options;
use rand::{distributions::Alphanumeric, seq::SliceRandom, Rng, RngCore};
use serde::{Deserialize, Serialize};

use crate::{
    meta::{deserialize, serialize, OpNumber},
    transport::Run,
    Client,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Read(String),
    Scan(String, usize),
    Update(String, String),
    Insert(String, String),
    Delete(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Result {
    ReadOk(String),
    ScanOk(Vec<String>),
    UpdateOk,
    InsertOk,
    DeleteOk,
    NotFound,
    // batched?
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct App(BTreeMap<String, String>);
impl crate::App for App {
    fn replica_upcall(&mut self, _: OpNumber, op: &[u8]) -> Vec<u8> {
        let Self(table) = self;
        let result = match deserialize(op) {
            Op::Read(key) => {
                if let Some(value) = table.get(&key).cloned() {
                    Result::ReadOk(value)
                } else {
                    Result::NotFound
                }
            }
            Op::Scan(key, count) => {
                let values = table
                    .range(key..)
                    .map(|(_, value)| value.clone())
                    .take(count)
                    .collect();
                Result::ScanOk(values)
            }
            Op::Update(key, value) => {
                if let Some(value_mut) = table.get_mut(&key) {
                    *value_mut = value;
                    Result::UpdateOk
                } else {
                    Result::NotFound
                }
            }
            Op::Insert(key, value) => {
                table.insert(key, value); // check for override?
                Result::InsertOk
            }
            Op::Delete(key) => {
                if table.remove(&key).is_some() {
                    Result::DeleteOk
                } else {
                    Result::NotFound
                }
            }
        };
        assert_ne!(result, Result::NotFound);
        let mut buf = Vec::new();
        serialize(&mut buf, result);
        buf
    }
}

pub struct Workload {
    keys: Vec<String>,
    values: Vec<String>,
    read_portion: u32,
    update_portion: u32,
    rmw_portion: u32,
}

impl Workload {
    fn iter_strings(rng_core: &mut impl RngCore, len: usize) -> impl Iterator<Item = String> + '_ {
        repeat_with(move || {
            rng_core
                .sample_iter(Alphanumeric)
                .take(len)
                .map(char::from)
                .collect()
        })
    }

    pub fn new_app(
        n_entries: usize,
        key_len: usize,
        value_len: usize,
        rng_core: &mut impl RngCore,
    ) -> App {
        let keys = Self::iter_strings(rng_core, key_len)
            .take(n_entries)
            .collect::<Vec<_>>();
        let entries = keys
            .into_iter()
            .zip(Self::iter_strings(rng_core, value_len))
            .collect();
        App(entries)
    }

    pub fn new(
        n_keys: usize,
        n_values: usize,
        key_len: usize,
        value_len: usize,
        read_portion: u32,
        update_portion: u32,
        rmw_portion: u32,
        rng_core: &mut impl RngCore,
    ) -> Self {
        let keys = Self::iter_strings(rng_core, key_len).take(n_keys).collect();
        let values = Self::iter_strings(rng_core, value_len)
            .take(n_values)
            .collect();
        Self {
            keys,
            values,
            read_portion,
            update_portion,
            rmw_portion,
        }
    }

    pub async fn invoke(&self, client: &mut (impl Client + Run), rng_core: &mut impl RngCore) {
        let ttype = rng_core.gen_range(0..100);
        if ttype < self.read_portion {
            // TODO zipf distribution
            let key = self.keys.choose(rng_core).unwrap();
            let invoke = client.invoke(
                &bincode::options()
                    .serialize(&Op::Read(key.clone()))
                    .unwrap(),
            );
            client
                .run(async {
                    invoke.await;
                })
                .await; // TODO verify result
        } else if ttype < self.read_portion + self.update_portion {
            let key = self.keys.choose(rng_core).unwrap();
            let value = self.values.choose(rng_core).unwrap();
            let invoke = client.invoke(
                &bincode::options()
                    .serialize(&Op::Update(key.clone(), value.clone()))
                    .unwrap(),
            );
            client
                .run(async {
                    invoke.await;
                })
                .await;
        } else {
            assert_eq!(
                self.read_portion + self.update_portion + self.rmw_portion,
                100
            );
            let key = self.keys.choose(rng_core).unwrap();
            let value = self.values.choose(rng_core).unwrap();
            let read = Op::Read(key.clone());
            let update = Op::Update(key.clone(), value.clone());
            let invoke = client.invoke(&bincode::options().serialize(&read).unwrap());
            client
                .run(async {
                    invoke.await;
                })
                .await;
            let invoke = client.invoke(&bincode::options().serialize(&update).unwrap());
            client
                .run(async {
                    invoke.await;
                })
                .await;
        }
    }
}
