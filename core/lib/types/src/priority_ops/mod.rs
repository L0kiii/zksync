//! Definition of zkSync network priority operations: operations initiated from the L1.

use ethabi::{decode, ParamType};
use num::{BigUint, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use zksync_basic_types::{Address, Log, H256, U256};
use zksync_crypto::params::{
    ACCOUNT_ID_BIT_WIDTH, BALANCE_BIT_WIDTH, ETH_ADDRESS_BIT_WIDTH, FR_ADDRESS_LEN,
    TOKEN_BIT_WIDTH, TX_TYPE_BIT_WIDTH,
};
use zksync_utils::BigUintSerdeAsRadix10Str;

use super::{
    operations::{DepositOp, FullExitOp},
    utils::h256_as_vec,
    AccountId, SerialId, TokenId,
};
use crate::priority_ops::error::LogParseError;
use zksync_crypto::primitives::FromBytes;

mod error;
#[cfg(test)]
mod tests;

/// Deposit priority operation transfers funds from the L1 account to the desired L2 account.
/// If the target L2 account didn't exist at the moment of the operation execution, a new
/// account will be created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    /// Address of the transaction initiator's L1 account.
    pub from: Address,
    /// Type of deposited token.
    pub token: TokenId,
    /// Amount of tokens deposited.
    #[serde(with = "BigUintSerdeAsRadix10Str")]
    pub amount: BigUint,
    /// Address of L2 account to deposit funds to.
    pub to: Address,
}

/// Performs a withdrawal of funds without direct interaction with the L2 network.
/// All the balance of the desired token will be withdrawn to the provided L1 address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullExit {
    pub account_id: AccountId,
    pub eth_address: Address,
    pub token: TokenId,
}

/// A set of L1 priority operations supported by the zkSync network.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ZkSyncPriorityOp {
    Deposit(Deposit),
    FullExit(FullExit),
}

impl ZkSyncPriorityOp {
    /// Attempts to interpret `ZkSyncPriorityOp` as `Deposit`.
    pub fn try_get_deposit(&self) -> Option<Deposit> {
        if let Self::Deposit(deposit) = self {
            Some(deposit.clone())
        } else {
            None
        }
    }

    /// Parses priority operation from the Ethereum logs.
    pub fn parse_from_priority_queue_logs(
        pub_data: &[u8],
        op_type_id: u8,
        sender: Address,
    ) -> Result<Self, LogParseError> {
        // see contracts/contracts/Operations.sol
        match op_type_id {
            DepositOp::OP_CODE => {
                let pub_data_left = pub_data;

                if pub_data_left.len() < TX_TYPE_BIT_WIDTH / 8 {
                    return Err(LogParseError::PubdataLengthMismatch);
                }
                let (_, pub_data_left) = pub_data_left.split_at(TX_TYPE_BIT_WIDTH / 8);

                // account_id
                if pub_data_left.len() < ACCOUNT_ID_BIT_WIDTH / 8 {
                    return Err(LogParseError::PubdataLengthMismatch);
                }
                let (_, pub_data_left) = pub_data_left.split_at(ACCOUNT_ID_BIT_WIDTH / 8);

                // token
                let (token, pub_data_left) = {
                    if pub_data_left.len() < TOKEN_BIT_WIDTH / 8 {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (token, left) = pub_data_left.split_at(TOKEN_BIT_WIDTH / 8);
                    (u16::from_be_bytes(token.try_into().unwrap()), left)
                };

                // amount
                let (amount, pub_data_left) = {
                    if pub_data_left.len() < BALANCE_BIT_WIDTH / 8 {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (amount, left) = pub_data_left.split_at(BALANCE_BIT_WIDTH / 8);
                    let amount = u128::from_be_bytes(amount.try_into().unwrap());
                    (BigUint::from(amount), left)
                };

                // account
                let (account, pub_data_left) = {
                    if pub_data_left.len() < FR_ADDRESS_LEN {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (account, left) = pub_data_left.split_at(FR_ADDRESS_LEN);
                    (Address::from_slice(account), left)
                };

                if !pub_data_left.is_empty() {
                    return Err(LogParseError::PubdataLengthMismatch);
                }

                Ok(Self::Deposit(Deposit {
                    from: sender,
                    token: TokenId(token),
                    amount,
                    to: account,
                }))
            }
            FullExitOp::OP_CODE => {
                if pub_data.len() < TX_TYPE_BIT_WIDTH / 8 {
                    return Err(LogParseError::PubdataLengthMismatch);
                }
                let (_, pub_data_left) = pub_data.split_at(TX_TYPE_BIT_WIDTH / 8);

                // account_id
                let (account_id, pub_data_left) = {
                    if pub_data_left.len() < ACCOUNT_ID_BIT_WIDTH / 8 {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (account_id, left) = pub_data_left.split_at(ACCOUNT_ID_BIT_WIDTH / 8);
                    (u32::from_bytes(account_id).unwrap(), left)
                };

                // owner
                let (eth_address, pub_data_left) = {
                    if pub_data_left.len() < ETH_ADDRESS_BIT_WIDTH / 8 {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (eth_address, left) = pub_data_left.split_at(ETH_ADDRESS_BIT_WIDTH / 8);
                    (Address::from_slice(eth_address), left)
                };

                // token
                let (token, pub_data_left) = {
                    if pub_data_left.len() < TOKEN_BIT_WIDTH / 8 {
                        return Err(LogParseError::PubdataLengthMismatch);
                    }
                    let (token, left) = pub_data_left.split_at(TOKEN_BIT_WIDTH / 8);
                    (u16::from_be_bytes(token.try_into().unwrap()), left)
                };

                // amount
                if pub_data_left.len() != BALANCE_BIT_WIDTH / 8 {
                    return Err(LogParseError::PubdataLengthMismatch);
                }

                Ok(Self::FullExit(FullExit {
                    account_id: AccountId(account_id),
                    eth_address,
                    token: TokenId(token),
                }))
            }
            _ => Err(LogParseError::UnsupportedPriorityOpType),
        }
    }

    /// Returns the amount of chunks required to include the priority operation into the block.
    pub fn chunks(&self) -> usize {
        match self {
            Self::Deposit(_) => DepositOp::CHUNKS,
            Self::FullExit(_) => FullExitOp::CHUNKS,
        }
    }

    /// Returns data needed to cancel priority queue events in exodus mode.
    fn get_args_for_priority_queue_cancel<'a, I: IntoIterator<Item = &'a Self> + 'a>(
        queue_entries: I,
    ) -> (u64, Vec<Vec<u8>>) {
        let mut n = 0;
        let mut deposits_data = Vec::new();
        for queue_entry in queue_entries.into_iter() {
            n += 1;
            if let Some(deposit) = queue_entry.try_get_deposit() {
                // Deposit pubdata for priority queue
                let mut data = vec![DepositOp::OP_CODE];
                data.extend_from_slice(&[0u8; 4]);
                data.extend_from_slice(&deposit.token.to_be_bytes());
                data.extend_from_slice(&deposit.amount.to_u128().unwrap().to_be_bytes());
                data.extend_from_slice(&deposit.to.as_bytes());
                deposits_data.push(data);
            }
        }
        deposits_data.resize(n as usize, Vec::new());
        (n, deposits_data)
    }
}

/// Priority operation description with the metadata required for server to process it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityOp {
    /// Unique ID of the priority operation.
    pub serial_id: SerialId,
    /// Priority operation.
    pub data: ZkSyncPriorityOp,
    /// Ethereum deadline block until which operation must be processed.
    pub deadline_block: u64,
    #[serde(with = "h256_as_vec")]
    /// Hash of the corresponding Ethereum transaction. Size should be 32 bytes
    pub eth_hash: H256,
    /// Block in which Ethereum transaction was included.
    pub eth_block: u64,
}

impl TryFrom<Log> for PriorityOp {
    type Error = LogParseError;

    fn try_from(event: Log) -> Result<PriorityOp, LogParseError> {
        let mut dec_ev = decode(
            &[
                ParamType::Address,
                ParamType::Uint(64),  // Serial id
                ParamType::Uint(8),   // OpType
                ParamType::Bytes,     // Pubdata
                ParamType::Uint(256), // expir. block
            ],
            &event.data.0,
        )?;

        let sender = dec_ev.remove(0).to_address().unwrap();
        Ok(PriorityOp {
            serial_id: dec_ev
                .remove(0)
                .to_uint()
                .as_ref()
                .map(U256::as_u64)
                .unwrap(),
            data: {
                let op_type = dec_ev
                    .remove(0)
                    .to_uint()
                    .as_ref()
                    .map(|ui| U256::as_u32(ui) as u8)
                    .unwrap();
                let op_pubdata = dec_ev.remove(0).to_bytes().unwrap();
                ZkSyncPriorityOp::parse_from_priority_queue_logs(&op_pubdata, op_type, sender)?
            },
            deadline_block: dec_ev
                .remove(0)
                .to_uint()
                .as_ref()
                .map(U256::as_u64)
                .unwrap(),
            eth_hash: event
                .transaction_hash
                .expect("Event transaction hash is missing"),
            eth_block: event
                .block_number
                .expect("Event block number is missing")
                .as_u64(),
        })
    }
}

impl PriorityOp {
    pub fn get_args_for_priority_queue_cancel(queue_entries: &[Self]) -> (u64, Vec<Vec<u8>>) {
        ZkSyncPriorityOp::get_args_for_priority_queue_cancel(
            queue_entries.iter().map(|priority_op| &priority_op.data),
        )
    }
}
